//! Bounded, read-only protocol inspection. No vault, signing, storage mutation or network access.
mod bitcoin;
mod evm;
use crate::{protocol, Error, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use serde_json::Value;

pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
const MAX_FIELDS: usize = 24_000;
const MAX_DOCUMENTS: usize = 32;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub source_sha256: String,
    pub source_size: usize,
    pub encoding: String,
    pub documents: Vec<Document>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub name: String,
    pub protocol: String,
    pub hex: String,
    pub fields: Vec<Field>,
    pub checks: Vec<Check>,
    pub properties: Vec<(String, String)>,
}
#[derive(Serialize)]
pub struct Field {
    pub offset: usize,
    pub length: usize,
    pub name: String,
    pub group: String,
    pub value: String,
    pub explanation: String,
    pub expected: String,
}
#[derive(Serialize)]
pub struct Check {
    pub status: &'static str,
    pub name: String,
    pub detail: String,
}
impl Document {
    fn new(name: &str, protocol: &str, bytes: &[u8]) -> Self {
        Self {
            name: name.into(),
            protocol: protocol.into(),
            hex: hex::encode(bytes),
            fields: vec![],
            checks: vec![],
            properties: vec![],
        }
    }
    fn field(
        &mut self,
        offset: usize,
        length: usize,
        name: impl Into<String>,
        group: &str,
        value: impl Into<String>,
        explanation: &str,
        expected: &str,
    ) -> std::result::Result<(), &'static str> {
        if self.fields.len() >= MAX_FIELDS {
            return Err(
                "Field limit reached; the remaining bytes are shown without detailed decoding.",
            );
        }
        if length > 0 {
            self.fields.push(Field {
                offset,
                length,
                name: name.into(),
                group: group.into(),
                value: value.into(),
                explanation: explanation.into(),
                expected: expected.into(),
            });
        }
        Ok(())
    }
    fn check(&mut self, status: &'static str, name: &str, detail: impl Into<String>) {
        self.checks.push(Check {
            status,
            name: name.into(),
            detail: detail.into(),
        });
    }
    fn finish(&mut self, bytes: &[u8], result: std::result::Result<(), &'static str>) {
        if let Err(message) = result {
            self.check("fail", "Decoding", message);
        }
        self.fields.sort_by_key(|f| f.offset);
        let end = self.fields.last().map_or(0, |f| f.offset + f.length);
        if end < bytes.len() {
            // One fallback field remains possible even when the detailed field budget is exhausted.
            self.fields.push(Field { offset: end, length: bytes.len() - end, name: "Undecoded bytes".into(), group: "opaque".into(), value: format!("{} bytes", bytes.len() - end), explanation: "These bytes have not been interpreted by this decoder.".into(), expected: "Use the protocol documentation to inspect these bytes; do not assume they are valid.".into() });
        }
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
type Decode<T> = std::result::Result<T, &'static str>;
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Decode<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or("Truncated field or length extends beyond the file.")?;
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }
    fn field(
        &mut self,
        d: &mut Document,
        n: usize,
        name: &str,
        group: &str,
        explanation: &str,
        expected: &str,
    ) -> Decode<&'a [u8]> {
        let offset = self.pos;
        let b = self.take(n)?;
        let value = if b.len() <= 80 {
            hex::encode(b)
        } else {
            format!("{} bytes; preview {}…", b.len(), hex::encode(&b[..32]))
        };
        d.field(offset, n, name, group, value, explanation, expected)?;
        Ok(b)
    }
    fn number(
        &mut self,
        d: &mut Document,
        n: usize,
        name: &str,
        group: &str,
        explanation: &str,
        expected: &str,
    ) -> Decode<u64> {
        let b = self.field(d, n, name, group, explanation, expected)?;
        let mut buf = [0u8; 8];
        buf[..n].copy_from_slice(b);
        let value = u64::from_le_bytes(buf);
        if let Some(f) = d.fields.last_mut() {
            f.value = value.to_string();
        }
        Ok(value)
    }
    fn compact(&mut self, d: &mut Document, name: &str, group: &str) -> Decode<usize> {
        let offset = self.pos;
        let tag = self.take(1)?[0];
        let n = match tag {
            253 => 2,
            254 => 4,
            255 => 8,
            _ => 0,
        };
        let value = if n == 0 {
            u64::from(tag)
        } else {
            let mut v = [0; 8];
            v[..n].copy_from_slice(self.take(n)?);
            u64::from_le_bytes(v)
        };
        d.field(
            offset,
            self.pos - offset,
            name,
            group,
            value.to_string(),
            "CompactSize is a little-endian variable-length count or byte length.",
            "Use the shortest encoding: one byte below 253, then fd + 2, fe + 4, or ff + 8 bytes.",
        )?;
        if (tag == 253 && value < 253)
            || (tag == 254 && value <= 0xffff)
            || (tag == 255 && value <= 0xffff_ffff)
        {
            return Err("Non-canonical CompactSize integer.");
        }
        usize::try_from(value).map_err(|_| "Length exceeds this platform's limits.")
    }
    fn end(&self) -> Decode<()> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err("Trailing bytes remain after the complete structure.")
        }
    }
}
/// Inspect bytes without importing them into a vault. Hint overrides ambiguous format detection only.
pub fn inspect(bytes: &[u8], hint: &str) -> Result<Report> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Error::Invalid("Inspector files must be 8 MiB or smaller"));
    }
    if !["auto", "bitcoin-tx", "psbt", "bitcoin-p2p", "evm"].contains(&hint) {
        return Err(Error::Invalid("Inspector decoder"));
    }
    let mut report = Report {
        source_sha256: protocol::digest(bytes),
        source_size: bytes.len(),
        encoding: "binary".into(),
        documents: vec![],
    };
    let text = std::str::from_utf8(bytes).ok().map(str::trim);
    if let Some(text) = text {
        if text.starts_with('{') {
            decode_document(&mut report.documents, bytes, "Original file", hint, 0);
            return Ok(report);
        }
        let compact: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        let hex_text = compact.strip_prefix("0x").unwrap_or(&compact);
        let decoded = if !hex_text.is_empty()
            && hex_text.len() % 2 == 0
            && hex_text.bytes().all(|b| b.is_ascii_hexdigit())
        {
            hex::decode(hex_text).ok().map(|b| ("hex text", b))
        } else if compact.len() >= 8 {
            STANDARD
                .decode(&compact)
                .ok()
                .filter(|b| known(b) || hint != "auto")
                .map(|b| ("base64 text", b))
        } else {
            None
        };
        if let Some((encoding, decoded)) = decoded {
            report.encoding = encoding.into();
            let mut source = Document::new("Original file", "Text encoding", bytes);
            let _ = source.field(0, bytes.len(), "Encoded text", "metadata", encoding, "This is the exact file content. The decoded document uses offsets in the decoded bytes.", "Hexadecimal pairs or standard Base64, optionally separated by whitespace.");
            report.documents.push(source);
            decode_document(&mut report.documents, &decoded, "Decoded payload", hint, 0);
            return Ok(report);
        }
    }
    decode_document(&mut report.documents, bytes, "Original file", hint, 0);
    Ok(report)
}
fn known(b: &[u8]) -> bool {
    b.starts_with(b"psbt\xff")
        || bitcoin::network_magic(b).is_some()
        || (b.len() >= 10 && matches!(b.get(..4), Some([1..=3, 0, 0, 0])))
        || evm::looks_like(b)
}
fn decode_document(docs: &mut Vec<Document>, bytes: &[u8], name: &str, hint: &str, depth: usize) {
    if docs.len() >= MAX_DOCUMENTS || depth > 3 {
        return;
    }
    if hint == "auto" && bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
        if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
            if value.is_object() {
                decode_json(docs, bytes, name, value, depth);
                return;
            }
        }
        let mut d = Document::new(name, "Unrecognized data", bytes);
        d.check(
            "fail",
            "JSON syntax",
            "The content starts like JSON but cannot be parsed as a complete JSON object.",
        );
        d.finish(bytes, Ok(()));
        docs.push(d);
        return;
    }
    let family = if hint != "auto" {
        hint
    } else if bytes.starts_with(b"psbt\xff") {
        "psbt"
    } else if bitcoin::network_magic(bytes).is_some() {
        "bitcoin-p2p"
    } else if bytes.len() >= 10 && matches!(bytes.get(..4), Some([1..=3, 0, 0, 0])) {
        "bitcoin-tx"
    } else if evm::looks_like(bytes) {
        "evm"
    } else {
        "unknown"
    };
    let protocol = match family {
        "psbt" => "Bitcoin PSBT",
        "bitcoin-p2p" => "Bitcoin P2P v1",
        "bitcoin-tx" => "Bitcoin transaction",
        "evm" => "EVM transaction",
        _ => "Unrecognized data",
    };
    let mut d = Document::new(name, protocol, bytes);
    let mut children = vec![];
    let result = match family {
        "bitcoin-tx" => bitcoin::transaction(&mut d, bytes),
        "psbt" => bitcoin::psbt(&mut d, bytes, &mut children),
        "bitcoin-p2p" => bitcoin::p2p(&mut d, bytes, &mut children),
        "evm" => evm::transaction(&mut d, bytes),
        _ => {
            d.check("info", "Format", "No supported protocol was recognized. The bytes remain available for inspection; a file extension does not establish its format.");
            Ok(())
        }
    };
    d.finish(bytes, result);
    docs.push(d);
    for (label, child, child_hint) in children.into_iter().take(16) {
        if docs.len() >= MAX_DOCUMENTS {
            break;
        }
        decode_document(docs, &child, &label, &child_hint, depth + 1);
    }
}
fn json_spans(bytes: &[u8]) -> Vec<(String, usize, usize)> {
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() && bytes[i] != b'{' {
        i += 1;
    }
    i += 1;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'"' {
            break;
        }
        let start = i;
        i += 1;
        let key_start = i;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
            } else if bytes[i] == b'"' {
                break;
            } else {
                i += 1;
            }
        }
        if i >= bytes.len() {
            break;
        }
        let key = serde_json::from_slice::<String>(&bytes[key_start - 1..=i]).unwrap_or_default();
        i += 1;
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b':') {
            i += 1;
        }
        let mut depth = 0usize;
        let mut quoted = false;
        while i < bytes.len() {
            let b = bytes[i];
            if quoted {
                if b == b'\\' {
                    i += 2;
                    continue;
                }
                if b == b'"' {
                    quoted = false;
                }
            } else {
                match b {
                    b'"' => quoted = true,
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' if depth > 0 => depth -= 1,
                    b',' | b'}' if depth == 0 => break,
                    _ => {}
                }
            }
            i += 1;
        }
        out.push((key, start, i));
        if out.len() >= MAX_FIELDS {
            break;
        }
    }
    out
}
fn decode_json(docs: &mut Vec<Document>, bytes: &[u8], name: &str, value: Value, depth: usize) {
    let format = value["format"].as_str().unwrap_or("");
    let envelope = matches!(format, "airvault-request" | "airvault-signature");
    let chain = value.get("operation").is_some() && value.get("sender").is_some();
    let mut d = Document::new(
        name,
        if envelope {
            "AirVault envelope"
        } else if chain {
            "AirVault chain transaction"
        } else {
            "JSON"
        },
        bytes,
    );
    let mut offset = 0;
    for (key, start, end) in json_spans(bytes) {
        let _ = d.field(
            offset,
            start - offset,
            "JSON delimiter",
            "metadata",
            "",
            "JSON object punctuation and whitespace.",
            "UTF-8 JSON.",
        );
        let (group,explain,expected)=match key.as_str() {
            "payload" => ("payload","Base64-encoded transaction or PSBT; select its decoded document to inspect individual bytes.","Encoding must be base64. The hash must match payloadSha256."),
            "payloadSha256" => ("metadata","SHA-256 of the decoded payload. This detects corruption; a matching hash does not authenticate the sender.","64 hexadecimal characters matching the decoded payload."),
            "requestDigest" => ("metadata","Binds a response to the complete original request.","Compare against the original request through signing review."),
            "network" => ("header","The declared blockchain/network; raw Bitcoin transactions do not encode a network identifier.","Must match the intended wallet and network."),
            "operation" => ("payload","The typed operation that AirVault will construct and sign.","Check destination, amount, fee and any token contract against an independent source."),
            _ => ("metadata","A JSON envelope or transaction property.","Verify this value against the intended operation.")
        };
        let preview = if key == "payload" {
            "Encoded payload".into()
        } else {
            value
                .get(&key)
                .map(|v| {
                    let s = v.to_string();
                    if s.len() > 180 {
                        format!("{}…", s.chars().take(120).collect::<String>())
                    } else {
                        s
                    }
                })
                .unwrap_or_default()
        };
        let _ = d.field(start, end - start, key, group, preview, explain, expected);
        offset = end;
    }
    let _ = d.field(
        offset,
        bytes.len() - offset,
        "JSON delimiter",
        "metadata",
        "",
        "Closing JSON punctuation and whitespace.",
        "UTF-8 JSON.",
    );
    d.check(
        "pass",
        "JSON syntax",
        "The complete document parses as a JSON object.",
    );
    if chain {
        d.check("info","Chain operation","Fields are shown as typed JSON. This view does not compile a serialized transaction or execute a contract.");
    }
    let mut payload = None;
    if envelope {
        let decoded = value["payload"]["data"]
            .as_str()
            .and_then(|s| STANDARD.decode(s).ok());
        if value["payload"]["encoding"] != "base64"
            || decoded
                .as_ref()
                .is_some_and(|b| b.len() > protocol::MAX_PAYLOAD_BYTES)
        {
            d.check(
                "fail",
                "Payload encoding",
                "The envelope must carry a bounded Base64 payload.",
            );
        } else if let Some(b) = decoded {
            let calculated = protocol::digest(&b);
            d.check(
                if value["payloadSha256"].as_str() == Some(&calculated) {
                    "pass"
                } else {
                    "fail"
                },
                "Payload SHA-256",
                format!("Calculated: {calculated}"),
            );
            let valid = if format == "airvault-request" {
                protocol::Request::parse(bytes).is_ok()
            } else {
                protocol::Response::parse(bytes).is_ok()
            };
            d.check(if valid {"pass"} else {"fail"},"Envelope schema","Checks format, version, fields and payload integrity using the native import parser.");
            if let Some(network) = value["network"].as_str() {
                d.properties
                    .push(("Declared network".into(), network.into()));
            }
            if format == "airvault-signature"
                && matches!(
                    value["network"].as_str(),
                    Some("bitcoin" | "testnet" | "testnet4" | "regtest" | "signet")
                )
            {
                match ::bitcoin::consensus::deserialize::<::bitcoin::Transaction>(&b) {
                    Ok(tx) => {
                        let calculated = tx.compute_txid().to_string();
                        d.check(
                            if value["txid"].as_str() == Some(&calculated) {
                                "pass"
                            } else {
                                "fail"
                            },
                            "Declared transaction ID",
                            format!("Calculated txid: {calculated}"),
                        );
                    }
                    Err(_) => d.check(
                        "fail",
                        "Signed transaction",
                        "The Bitcoin response payload is not a canonical transaction.",
                    ),
                }
            }
            d.check("info","Request binding","For responses, requestDigest and txid must also be checked against the original request by signing review before broadcasting.");
            payload = Some(b);
        } else {
            d.check(
                "fail",
                "Payload encoding",
                "Payload is missing or is not standard Base64.",
            );
        }
    } else {
        d.check(
            "info",
            "Protocol validation",
            "JSON syntax alone does not establish a supported cryptocurrency datagram.",
        );
    }
    if d.fields.len() >= MAX_FIELDS {
        d.check(
            "info",
            "Field limit",
            "Remaining JSON fields are preserved as undecoded bytes.",
        );
    }
    d.finish(bytes, Ok(()));
    docs.push(d);
    if let Some(b) = payload {
        decode_document(docs, &b, "Envelope payload", "auto", depth + 1);
    }
}
