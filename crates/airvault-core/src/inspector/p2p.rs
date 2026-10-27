use super::{Children, Decode, Document, Reader};
use bitcoin::{
    consensus,
    hashes::{sha256d, Hash},
    p2p::message::RawNetworkMessage,
    Block,
};

pub fn network_magic(bytes: &[u8]) -> Option<&'static str> {
    match bytes.get(..4)? {
        [0xf9, 0xbe, 0xb4, 0xd9] => Some("Bitcoin mainnet"),
        [0x0b, 0x11, 0x09, 0x07] => Some("Bitcoin testnet3"),
        [0x1c, 0x16, 0x3f, 0x28] => Some("Bitcoin testnet4"),
        [0x0a, 0x03, 0xcf, 0x40] => Some("Bitcoin signet (default challenge)"),
        [0xfa, 0xbf, 0xb5, 0xda] => Some("Bitcoin regtest"),
        _ => None,
    }
}
fn header(r: &mut Reader<'_>, d: &mut Document, label: &str) -> Decode<()> {
    for (n, name, explanation) in [
        (4, "version", "Block version and version-bits signaling."),
        (
            32,
            "previous block",
            "Previous block hash in internal byte order.",
        ),
        (
            32,
            "merkle root",
            "Commits to the block's non-witness transaction IDs.",
        ),
        (
            4,
            "timestamp",
            "Block Unix timestamp, encoded little-endian.",
        ),
        (
            4,
            "difficulty bits",
            "Compact encoding of the proof-of-work target.",
        ),
        (
            4,
            "nonce",
            "Value miners vary when searching for proof of work.",
        ),
    ] {
        r.field(d,n,&format!("{label} · {name}"),"header",explanation,"Must agree with the intended chain and block. Header decoding does not establish chain membership.")?;
    }
    Ok(())
}
fn body(
    r: &mut Reader<'_>,
    d: &mut Document,
    command: &str,
    children: &mut Children,
    index: usize,
) -> Decode<()> {
    match command {
        "tx" => {
            let start = r.pos;
            super::transaction_fields(d, r)?;
            if children.len() < 16 {
                children.push((
                    format!("Message {index} · transaction"),
                    r.bytes[start..r.pos].to_vec(),
                    "bitcoin-tx".into(),
                ));
            }
        }
        "block" => {
            let start = r.pos;
            header(r, d, "Block header")?;
            let count = r.compact(d, "Block transaction count", "metadata")?;
            if count == 0 || count > 10_000 {
                return Err("Block transaction count exceeds the inspector limit or is zero.");
            }
            for i in 0..count {
                let txstart = r.pos;
                // Keep a single set of document properties for the enclosing block.
                let property_count = d.properties.len();
                super::transaction_fields(d, r)?;
                d.properties.truncate(property_count);
                if children.len() < 16 {
                    children.push((
                        format!("Block transaction {}", i + 1),
                        r.bytes[txstart..r.pos].to_vec(),
                        "bitcoin-tx".into(),
                    ));
                }
            }
            let block: Block = consensus::deserialize(&r.bytes[start..r.pos])
                .map_err(|_| "Invalid serialized Bitcoin block.")?;
            d.check(
                if block.check_merkle_root() {
                    "pass"
                } else {
                    "fail"
                },
                "Block merkle root",
                "Recomputed the transaction-ID merkle root and compared it with the header.",
            );
            d.check(
                if block.check_witness_commitment() {
                    "pass"
                } else {
                    "fail"
                },
                "Block witness commitment",
                "Checked the witness commitment when witness transactions are present.",
            );
            d.properties
                .push(("Block hash".into(), block.block_hash().to_string()));
            d.check("info","Block validation","Proof of work, active-chain membership and transaction scripts are not validated here.");
        }
        "ping" | "pong" => {
            r.number(
                d,
                8,
                "Ping/pong nonce",
                "metadata",
                "Opaque nonce used to match a ping with its pong reply.",
                "The responding pong must repeat the ping's 64-bit nonce.",
            )?;
        }
        "feefilter" => {
            r.number(
                d,
                8,
                "Fee filter (sats/kvB)",
                "metadata",
                "The peer's minimum accepted fee rate.",
                "Non-negative fee rate; this is peer policy, not a fee paid by a transaction.",
            )?;
        }
        "sendcmpct" => {
            r.number(
                d,
                1,
                "Compact-block announcements",
                "metadata",
                "Whether compact-block announcements are requested.",
                "0 or 1.",
            )?;
            r.number(
                d,
                8,
                "Compact-block version",
                "metadata",
                "Compact-block protocol version requested by the peer.",
                "A supported compact-block version.",
            )?;
        }
        "verack" | "getaddr" | "mempool" | "sendheaders" | "wtxidrelay" | "sendaddrv2" => {}
        "inv" | "getdata" | "notfound" => {
            let count = r.compact(d, "Inventory count", "metadata")?;
            if count > 50_000 {
                return Err("Inventory exceeds the 50,000-entry protocol limit.");
            }
            for i in 0..count {
                r.number(
                    d,
                    4,
                    &format!("Inventory {} · type", i + 1),
                    "metadata",
                    "Object type and optional witness flag.",
                    "1 is transaction, 2 is block; 0x40000000 requests witness serialization.",
                )?;
                r.field(
                    d,
                    32,
                    &format!("Inventory {} · hash", i + 1),
                    "payload",
                    "Object identifier in internal byte order.",
                    "Must identify the requested or announced transaction/block.",
                )?;
            }
        }
        "headers" => {
            let count = r.compact(d, "Header count", "metadata")?;
            if count > 2000 {
                return Err("Headers message exceeds 2,000 entries.");
            }
            for i in 0..count {
                header(r, d, &format!("Header {}", i + 1))?;
                if r.compact(d, "Header transaction count", "metadata")? != 0 {
                    return Err("Headers entries must have a zero transaction count.");
                }
            }
            d.check(
                "info",
                "Header validation",
                "Header fields are decoded; proof of work and chain linkage are not established.",
            );
        }
        "getheaders" | "getblocks" => {
            r.number(
                d,
                4,
                "Protocol version",
                "header",
                "The requesting peer's protocol version.",
                "Must be compatible with the peer.",
            )?;
            let count = r.compact(d, "Block locator count", "metadata")?;
            if count > 101 {
                return Err("Block locator exceeds 101 hashes.");
            }
            for i in 0..count {
                r.field(
                    d,
                    32,
                    &format!("Locator {}", i + 1),
                    "payload",
                    "Known ancestor block hash in internal byte order.",
                    "Ordered from recent blocks to older ancestors.",
                )?;
            }
            r.field(
                d,
                32,
                "Stop hash",
                "payload",
                "Optional last requested block; zero means no explicit stop.",
                "A block hash in internal byte order, or all zero bytes.",
            )?;
        }
        "version" => {
            let version = r.number(
                d,
                4,
                "Protocol version",
                "header",
                "The peer's supported protocol version.",
                "A signed 32-bit little-endian protocol version.",
            )?;
            r.number(
                d,
                8,
                "Services",
                "metadata",
                "Bit flags describing peer capabilities.",
                "Declared capabilities must be supported by the peer.",
            )?;
            r.number(
                d,
                8,
                "Timestamp",
                "metadata",
                "The peer's reported Unix time.",
                "A plausible timestamp; it is not trusted time.",
            )?;
            address(r, d, "Receiver")?;
            if version >= 106 {
                address(r, d, "Sender")?;
                r.number(
                    d,
                    8,
                    "Connection nonce",
                    "metadata",
                    "Random identifier used to detect self-connections.",
                    "Must differ between unrelated connections.",
                )?;
                let n = r.compact(d, "User agent length", "metadata")?;
                if n > 256 {
                    return Err("Version user agent exceeds 256 bytes.");
                }
                r.field(
                    d,
                    n,
                    "User agent",
                    "metadata",
                    "Self-reported software identifier; arbitrary untrusted text.",
                    "Usually a slash-delimited agent string. It does not authenticate software.",
                )?;
                r.number(
                    d,
                    4,
                    "Starting height",
                    "metadata",
                    "Peer's reported best block height.",
                    "A plausible signed height; not proof of synchronization.",
                )?;
                if r.pos < r.bytes.len() {
                    let relay = r.number(
                        d,
                        1,
                        "Relay transactions",
                        "metadata",
                        "Whether the peer wants transaction announcements.",
                        "0 or 1.",
                    )?;
                    if relay > 1 {
                        return Err("Relay flag must be 0 or 1.");
                    }
                }
            }
        }
        _ => {
            let n = r.bytes.len() - r.pos;
            r.field(d,n,"Message payload","opaque","Payload of a P2P command without a field-level decoder in this inspector.","Its transport checksum is checked, but the payload's protocol meaning is not validated.")?;
            d.check("info","Command payload","Transport framing is decoded. This command's payload is preserved as opaque bytes.");
        }
    }
    r.end()
}
fn address(r: &mut Reader<'_>, d: &mut Document, name: &str) -> Decode<()> {
    r.number(
        d,
        8,
        &format!("{name} services"),
        "metadata",
        "Service capability flags associated with this peer address.",
        "64-bit little-endian flags.",
    )?;
    r.field(
        d,
        16,
        &format!("{name} IP address"),
        "metadata",
        "IPv6 address or IPv4-mapped IPv6 address.",
        "16 network-order address bytes.",
    )?;
    r.field(
        d,
        2,
        &format!("{name} port"),
        "metadata",
        "TCP port, encoded in big-endian network byte order.",
        "Must match the intended peer service.",
    )?;
    Ok(())
}
pub fn p2p(d: &mut Document, bytes: &[u8], children: &mut Children) -> Decode<()> {
    let mut r = Reader::new(bytes);
    let mut count = 0;
    while r.pos < bytes.len() {
        count += 1;
        if count > 1024 {
            return Err("Message count exceeds the inspector limit of 1,024.");
        }
        let start = r.pos;
        let magic = r.field(
            d,
            4,
            &format!("Message {count} · network magic"),
            "header",
            "Identifies the network of this unencrypted Bitcoin v1 P2P message.",
            "Must match mainnet, testnet3, testnet4, default signet or regtest.",
        )?;
        let network=network_magic(magic).ok_or("Unknown network magic. Encrypted v2 transport and custom signet magic are not decoded.")?;
        let command = r.field(
            d,
            12,
            &format!("Message {count} · command"),
            "header",
            "ASCII message command, padded to 12 bytes with zeroes.",
            "Printable ASCII followed only by zero padding.",
        )?;
        let end = command.iter().position(|b| *b == 0).unwrap_or(12);
        if end == 0
            || !command[..end].iter().all(|b| (0x20..=0x7e).contains(b))
            || command[end..].iter().any(|b| *b != 0)
        {
            return Err("Invalid P2P command or non-zero command padding.");
        }
        let command = std::str::from_utf8(&command[..end])
            .map_err(|_| "Invalid command.")?
            .to_string();
        if let Some(f) = d.fields.last_mut() {
            f.value = command.clone();
        }
        let len = r.number(
            d,
            4,
            &format!("Message {count} · payload length"),
            "header",
            "Number of payload bytes after the 24-byte message header.",
            "Must match the complete message payload; subsequent messages have their own headers.",
        )? as usize;
        let checksum=r.field(d,4,&format!("Message {count} · checksum"),"header","First four bytes of SHA256(SHA256(payload)). Detects corruption, not sender authenticity.","Must equal the first four bytes of the independently calculated double SHA-256.")?;
        let payload_start = r.pos;
        let payload = r.take(len)?;
        let calculated = sha256d::Hash::hash(payload).to_byte_array();
        d.check(
            if checksum == &calculated[..4] {
                "pass"
            } else {
                "fail"
            },
            &format!("Message {count} · checksum"),
            format!(
                "Expected {} · calculated {}",
                hex::encode(checksum),
                hex::encode(&calculated[..4])
            ),
        );
        d.properties.push((
            format!("Message {count}"),
            format!("{network} · {command} · {len} payload bytes"),
        ));
        // Use whole-file coordinates while bounding every body read at the declared payload end.
        let mut b = Reader {
            bytes: &bytes[..r.pos],
            pos: payload_start,
        };
        let result = body(&mut b, d, &command, children, count);
        if let Err(message) = result {
            d.check("fail", &format!("Message {count} · payload"), message);
            let last = d
                .fields
                .last()
                .map_or(payload_start, |f| f.offset + f.length);
            if last < r.pos {
                d.field(
                    last,
                    r.pos - last,
                    "Undecoded message payload",
                    "opaque",
                    "",
                    "The payload decoder stopped at this position.",
                    "Inspect the raw bytes and reported error.",
                )?;
            }
        } else if checksum == &calculated[..4] {
            // A separate consensus parser checks supported commands in addition to byte-range decoding.
            let parsed = consensus::deserialize::<RawNetworkMessage>(&bytes[start..r.pos]);
            d.check(
                if parsed.is_ok() { "pass" } else { "fail" },
                &format!("Message {count} · native parser"),
                "Independent Bitcoin P2P message parser, including declared framing and checksum.",
            );
        }
    }
    d.check("info","Transport scope","Unencrypted Bitcoin v1 messages only. Checksums do not authenticate peers. Encrypted BIP324 streams, packet capture containers and TCP/IP headers require external framing/decryption.");
    Ok(())
}
