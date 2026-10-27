use super::{Children, Decode, Document, Reader};
use bitcoin::{consensus, psbt::Psbt, Transaction};
use std::collections::HashSet;

fn key_name(scope: &str, t: u8) -> (&'static str, &'static str) {
    match (scope,t) {
        ("Global",0x00)=>("Unsigned transaction","The complete unsigned legacy-format transaction. Input scriptSigs and witnesses must be empty."),
        ("Global",0x01)=>("Extended public key","Public derivation information. This may expose wallet address relationships."),
        ("Global",0xfb)=>("PSBT version","Four-byte little-endian PSBT version. This decoder semantically validates version 0."),
        ("Input",0x00)=>("Previous transaction","Full transaction being spent. Its txid and output index must match this input."),
        ("Input",0x01)=>("Witness UTXO","Value and locking script of the spent output; must agree with any full previous transaction."),
        ("Input",0x02)=>("Partial signature","Public-key-keyed DER signature plus a sighash byte. Its presence does not prove it is valid."),
        ("Input",0x03)=>("Sighash type","Determines which transaction fields the signature commits to. SIGHASH_ALL is the normal complete-transaction choice."),
        ("Input",0x04)|("Output",0x00)=>("Redeem script","Must hash to the P2SH locking script when that output uses P2SH."),
        ("Input",0x05)|("Output",0x01)=>("Witness script","Must hash to the P2WSH witness program when that output uses P2WSH."),
        ("Input",0x06)|("Output",0x02)=>("BIP32 derivation","Public key origin fingerprint and path; verify against the independently registered wallet."),
        ("Input",0x07)=>("Final scriptSig","Final serialized unlocking script."),
        ("Input",0x08)=>("Final witness","Final serialized witness stack."),
        ("Input",0x13)=>("Taproot key signature","Schnorr key-path signature. Taproot execution is not performed by this inspector."),
        ("Input",0x14)=>("Taproot script signature","Schnorr signature associated with a public key and leaf hash."),
        ("Input",0x15)=>("Taproot leaf script","Script, leaf version and control block used for a script-path spend."),
        ("Input",0x16)|("Output",0x07)=>("Taproot derivation","Taproot leaf hashes, fingerprint and derivation path."),
        ("Input",0x17)|("Output",0x05)=>("Taproot internal key","The x-only internal public key used to construct a Taproot output."),
        (_,0xfc)=>("Proprietary record","Vendor-specific extension. Its bytes are preserved but its meaning is not trusted."),
        _=>("Extension record","An extension not semantically interpreted by this inspector. Preserve it when exchanging PSBTs.")
    }
}
fn map(
    r: &mut Reader<'_>,
    d: &mut Document,
    scope: &str,
    index: usize,
    children: &mut Children,
) -> Decode<Vec<(Vec<u8>, Vec<u8>)>> {
    let label = if scope == "Global" {
        "Global".into()
    } else {
        format!("{scope} {}", index + 1)
    };
    let group = match scope {
        "Input" => "input",
        "Output" => "output",
        _ => "metadata",
    };
    let mut entries = vec![];
    let mut seen = HashSet::new();
    loop {
        let n = r.compact(d, &format!("{label} · key length / separator"), group)?;
        if n == 0 {
            break;
        }
        let key=r.field(d,n,&format!("{label} · key"),group,"The key type followed by optional key data. Keys must be unique within each map.","The first CompactSize integer selects the record type. Public-key keyed records include key data.")?.to_vec();
        if !seen.insert(key.clone()) {
            return Err("Duplicate PSBT key in the same map.");
        }
        let (name, help) = key_name(scope, key[0]);
        let n = r.compact(d, &format!("{label} · {name} length"), group)?;
        let value = r
            .field(
                d,
                n,
                &format!("{label} · {name}"),
                group,
                help,
                "Must have the length and format required by its key type.",
            )?
            .to_vec();
        if key.len() == 1
            && ((scope == "Global" && key[0] == 0) || (scope == "Input" && key[0] == 0))
            && children.len() < 16
        {
            children.push((
                format!("{label} · {name}"),
                value.clone(),
                "bitcoin-tx".into(),
            ));
        }
        entries.push((key, value));
    }
    Ok(entries)
}
pub fn psbt(d: &mut Document, bytes: &[u8], children: &mut Children) -> Decode<()> {
    let mut r = Reader::new(bytes);
    let magic = r.field(
        d,
        5,
        "PSBT magic",
        "header",
        "Identifies the Partially Signed Bitcoin Transaction container.",
        "Must be 70 73 62 74 ff (psbt followed by ff).",
    )?;
    if magic != b"psbt\xff" {
        return Err("Invalid PSBT magic bytes.");
    }
    let global = map(&mut r, d, "Global", 0, children)?;
    if let Some((_, v)) = global.iter().find(|(k, _)| k == &[0xfb]) {
        if v.len() != 4 {
            return Err("PSBT version must be four bytes.");
        }
        let version = u32::from_le_bytes(v.as_slice().try_into().unwrap());
        d.properties
            .push(("PSBT version".into(), version.to_string()));
        if version != 0 {
            d.check("info","PSBT version","Only version 0 transaction semantics are decoded. The global map and remaining bytes are available, but this is not a validity check for PSBTv2.");
            return Ok(());
        }
    }
    let txbytes = global
        .iter()
        .find(|(k, _)| k == &[0])
        .ok_or("PSBTv0 is missing its unsigned transaction.")?;
    let tx: Transaction = consensus::deserialize(&txbytes.1)
        .map_err(|_| "Invalid PSBT unsigned transaction encoding.")?;
    if tx.input.len() > 10_000 || tx.output.len() > 10_000 {
        return Err("PSBT input/output count exceeds the inspector limit.");
    }
    for i in 0..tx.input.len() {
        map(&mut r, d, "Input", i, children)?;
    }
    for i in 0..tx.output.len() {
        map(&mut r, d, "Output", i, children)?;
    }
    r.end()?;
    let psbt = Psbt::deserialize(bytes)
        .map_err(|_| "Invalid PSBTv0 record, key, value, unsigned transaction or map count.")?;
    d.check("pass","PSBTv0 encoding","Native PSBT parser verified record types, map counts, key uniqueness, unsigned transaction and complete consumption.");
    d.check("info","Checksum","PSBT has no embedded file checksum. Compare the file SHA-256 with a trusted copy or verify the surrounding AirVault envelope.");
    d.properties.extend([
        ("Unsigned txid".into(), tx.compute_txid().to_string()),
        ("Inputs".into(), tx.input.len().to_string()),
        ("Outputs".into(), tx.output.len().to_string()),
    ]);
    let mut seen_outpoints = HashSet::new();
    if tx.input.is_empty()
        || tx.output.is_empty()
        || tx
            .input
            .iter()
            .any(|i| !seen_outpoints.insert(i.previous_output))
    {
        d.check(
            "fail",
            "Unsigned transaction inputs",
            "Inputs and outputs must be non-empty, and an outpoint cannot appear twice.",
        );
    }
    if tx
        .output
        .iter()
        .try_fold(0u64, |sum, o| {
            sum.checked_add(o.value.to_sat())
                .filter(|v| *v <= super::MAX_MONEY)
        })
        .is_none()
    {
        d.check(
            "fail",
            "Output money range",
            "Output values and their sum must not exceed 21 million BTC.",
        );
    }
    let mut all_values = true;
    let mut total = 0u64;
    for (i, input) in psbt.inputs.iter().enumerate() {
        let label = format!("Input {} · UTXO binding", i + 1);
        let mut previous = None;
        if let Some(prev) = &input.non_witness_utxo {
            let outpoint = tx.input[i].previous_output;
            if prev.compute_txid() != outpoint.txid || outpoint.vout as usize >= prev.output.len() {
                d.check(
                    "fail",
                    &label,
                    "The previous transaction hash or output index does not match the input.",
                );
                all_values = false;
                continue;
            }
            previous = prev.output.get(outpoint.vout as usize);
            d.check(
                "pass",
                &label,
                "Previous transaction txid and output index match this input.",
            );
        }
        if let (Some(a), Some(b)) = (previous, input.witness_utxo.as_ref()) {
            d.check(
                if a == b { "pass" } else { "fail" },
                &format!("Input {} · UTXO consistency", i + 1),
                "Witness UTXO must equal the referenced full previous output.",
            );
            if a != b {
                all_values = false;
            }
        }
        let output = previous.or(input.witness_utxo.as_ref());
        if let Some(output) = output {
            if let Some(sum) = total
                .checked_add(output.value.to_sat())
                .filter(|v| *v <= super::MAX_MONEY)
            {
                total = sum;
            } else {
                all_values = false;
                d.check(
                    "fail",
                    "Input money range",
                    "Supplied input values and their sum must not exceed 21 million BTC.",
                );
            }
            let locking = &output.script_pubkey;
            if let Some(redeem) = &input.redeem_script {
                d.check(
                    if redeem.to_p2sh() == *locking {
                        "pass"
                    } else {
                        "fail"
                    },
                    &format!("Input {} · redeem script hash", i + 1),
                    "HASH160(redeem script) must match the spent P2SH output.",
                );
            }
            if let Some(script) = &input.witness_script {
                let program = input.redeem_script.as_ref().unwrap_or(locking);
                d.check(
                    if script.to_p2wsh() == *program {
                        "pass"
                    } else {
                        "fail"
                    },
                    &format!("Input {} · witness script hash", i + 1),
                    "SHA-256(witness script) must match the spent P2WSH program.",
                );
            }
        } else {
            all_values = false;
        }
    }
    let output_sum = tx
        .output
        .iter()
        .try_fold(0u64, |sum, o| sum.checked_add(o.value.to_sat()));
    if all_values {
        if let Some(fee) = output_sum.and_then(|sum| total.checked_sub(sum)) {
            d.properties
                .push(("Fee from supplied UTXOs (sats)".into(), fee.to_string()));
        } else {
            d.check(
                "fail",
                "Amounts",
                "Output sum exceeds supplied input values or overflows.",
            );
        }
    } else {
        d.check("info","Fee","A reliable fee cannot be calculated without consistent previous-output values for every input.");
    }
    d.check("info","Signatures and ownership","This decoder does not execute PSBT signatures or establish wallet ownership. AirVault signing review independently checks eligible inputs against registered accounts before signing or broadcasting.");
    Ok(())
}
