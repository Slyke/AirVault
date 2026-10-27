use super::{Decode, Document, Reader};
use bitcoin::{consensus, Transaction};
use std::collections::HashSet;
#[path = "p2p.rs"]
mod p2p_decoder;
#[path = "psbt.rs"]
mod psbt_decoder;
pub use p2p_decoder::{network_magic, p2p};
pub use psbt_decoder::psbt;
pub type Children = Vec<(String, Vec<u8>, String)>;
const MAX_MONEY: u64 = 2_100_000_000_000_000;

fn script(r: &mut Reader<'_>, d: &mut Document, name: &str, group: &str) -> Decode<()> {
    let len = r.compact(d, &format!("{name} length"), group)?;
    let b=r.field(d,len,name,group,"A Bitcoin Script program or unlocking script. Script hashes constrain who may spend an output.","Check the destination locking script against the expected address; unfamiliar scripts require extra review.")?;
    if len > 0 {
        let s = bitcoin::Script::from_bytes(b);
        if let Some(f) = d.fields.last_mut() {
            f.value = format!(
                "{} · {}",
                if s.is_p2wpkh() {
                    "P2WPKH"
                } else if s.is_p2wsh() {
                    "P2WSH"
                } else if s.is_p2tr() {
                    "P2TR"
                } else if s.is_p2pkh() {
                    "P2PKH"
                } else if s.is_p2sh() {
                    "P2SH"
                } else if s.is_op_return() {
                    "OP_RETURN"
                } else {
                    "Script"
                },
                s.to_asm_string()
            );
            if f.value.len() > 2000 {
                f.value = f.value.chars().take(2000).collect();
            }
        }
    }
    Ok(())
}
pub fn transaction(d: &mut Document, bytes: &[u8]) -> Decode<()> {
    let mut r = Reader::new(bytes);
    transaction_fields(d, &mut r)?;
    r.end()?;
    let tx: Transaction = consensus::deserialize(bytes)
        .map_err(|_| "The transaction is not a canonical Bitcoin serialization.")?;
    d.check(
        "pass",
        "Transaction encoding",
        "Canonical transaction encoding, counts, byte lengths and complete consumption.",
    );
    d.check("info","Checksum","Raw Bitcoin transactions have no embedded file checksum. The txid and wtxid identify transaction content; compare them with a trusted copy.");
    d.check("info","Spend verification","This view does not execute scripts or query spent-output status, confirmations or fees. A decoded transaction is not proof that it can be spent or broadcast.");
    d.properties.extend([
        ("txid".into(), tx.compute_txid().to_string()),
        ("wtxid".into(), tx.compute_wtxid().to_string()),
        ("Virtual size".into(), tx.vsize().to_string()),
    ]);
    let mut seen = HashSet::new();
    let duplicate = tx.input.iter().any(|i| !seen.insert(i.previous_output));
    d.check(
        if duplicate { "fail" } else { "pass" },
        "Input uniqueness",
        if duplicate {
            "The same outpoint appears more than once."
        } else {
            "No duplicate outpoints."
        },
    );
    Ok(())
}
pub(super) fn transaction_fields(d: &mut Document, r: &mut Reader<'_>) -> Decode<()> {
    let version = r.number(
        d,
        4,
        "Version",
        "header",
        "Signed 32-bit transaction version, serialized little-endian.",
        "Common transaction versions are 1, 2 and 3. Version alone does not establish validity.",
    )?;
    d.properties
        .push(("Version".into(), (version as u32 as i32).to_string()));
    let witness = r.bytes.get(r.pos) == Some(&0);
    if witness {
        r.number(
            d,
            1,
            "Witness marker",
            "header",
            "Separates SegWit serialization from legacy transaction serialization.",
            "Must be 0.",
        )?;
        let flag = r.number(
            d,
            1,
            "Witness flag",
            "header",
            "Signals the presence of per-input witness stacks.",
            "Currently supported flag is 1.",
        )?;
        if flag != 1 {
            return Err("Unknown witness flag.");
        }
    }
    let inputs = r.compact(d, "Input count", "header")?;
    if inputs == 0 || inputs > 10_000 {
        return Err("Input count must be between 1 and the inspector limit of 10,000.");
    }
    for i in 0..inputs {
        let b=r.field(d,32,&format!("Input {} · previous txid",i+1),"input","Identifies the transaction whose output is being spent. Stored in internal byte order.","Reverse these 32 bytes to compare with the displayed transaction ID.")?;
        if let Some(f) = d.fields.last_mut() {
            f.value = hex::encode(b.iter().rev().copied().collect::<Vec<_>>());
        }
        r.number(
            d,
            4,
            &format!("Input {} · output index", i + 1),
            "input",
            "Zero-based index of the previous transaction's output.",
            "Must reference the intended existing output. Coinbase uses ffffffff.",
        )?;
        script(r, d, &format!("Input {} · scriptSig", i + 1), "input")?;
        r.number(
            d,
            4,
            &format!("Input {} · sequence", i + 1),
            "input",
            "Controls relative locktime and can signal opt-in replaceability.",
            "ffffffff is final; other values may enable locktime, relative locks or replacement.",
        )?;
    }
    let outputs = r.compact(d, "Output count", "header")?;
    if outputs == 0 || outputs > 10_000 {
        return Err("Output count must be between 1 and the inspector limit of 10,000.");
    }
    let mut total = 0u64;
    for i in 0..outputs {
        let amount=r.number(d,8,&format!("Output {} · amount (sats)",i+1),"output","The number of satoshis assigned to this output, as an unsigned little-endian integer.","Check the amount and its locking script. Each value and their sum must not exceed 21 million BTC.")?;
        total = total
            .checked_add(amount)
            .filter(|v| *v <= MAX_MONEY)
            .ok_or("Output amount or total exceeds Bitcoin's money range.")?;
        script(r, d, &format!("Output {} · scriptPubKey", i + 1), "output")?;
    }
    if witness {
        for i in 0..inputs {
            let items = r.compact(
                d,
                &format!("Input {} · witness item count", i + 1),
                "witness",
            )?;
            if items > 10_000 {
                return Err("Witness stack exceeds the inspector item limit.");
            }
            for j in 0..items {
                let n = r.compact(
                    d,
                    &format!("Input {} · witness {} length", i + 1, j + 1),
                    "witness",
                )?;
                r.field(d,n,&format!("Input {} · witness {}",i+1,j+1),"witness","An unlocking witness item, such as a signature, public key, witness script or Taproot control block.","Meaning depends on the spent output. Displaying an item does not verify its signature.")?;
            }
        }
    }
    r.number(d,4,"Locktime","header","Earliest eligible block height, or Unix timestamp when at least 500000000. Input sequences determine whether it is enabled.","0 means no absolute transaction locktime. Verify any requested delay.")?;
    d.properties.extend([
        ("Inputs".into(), inputs.to_string()),
        ("Outputs".into(), outputs.to_string()),
        ("Output total (sats)".into(), total.to_string()),
    ]);
    Ok(())
}
