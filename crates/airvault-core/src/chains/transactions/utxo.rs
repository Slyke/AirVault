use super::{fields, secp_verify, Details, Transaction as Draft};
use crate::{
    chains::{self, invalid, keys, Context},
    model::Network,
    Error, Result,
};
use bitcoin::{
    absolute, consensus,
    hashes::{hash160, sha256d, Hash},
    psbt::Psbt,
    script::{Builder, PushBytesBuf},
    sighash::SighashCache,
    transaction, Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::HashSet};
use tw_coin_entry::coin_entry::CoinEntry;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Transfer {
    pub to: String,
    pub amount: String,
    pub fee: String,
    pub utxos: Vec<crate::bitcoin::Utxo>,
}
pub fn script(address: &str, network: Network) -> Result<ScriptBuf> {
    let data = keys::address_bytes(address, network)?;
    let hash = if data.len() == 21 {
        &data[1..]
    } else {
        data.as_slice()
    };
    if hash.len() != 20 {
        return Err(Error::Invalid("P2PKH address required"));
    }
    if network == Network::BitcoinCash && data[0] == 5 {
        return Ok(ScriptBuf::new_p2sh(
            &bitcoin::ScriptHash::from_slice(hash).map_err(invalid)?,
        ));
    }
    Ok(ScriptBuf::new_p2pkh(
        &bitcoin::PubkeyHash::from_slice(hash).map_err(invalid)?,
    ))
}
fn build(t: &Transfer, tx: &Draft, network: Network) -> Result<Psbt> {
    if t.utxos.is_empty() || t.utxos.len() > 100 {
        return Err(Error::Invalid("input count"));
    }
    let amount: u64 = chains::units(&t.amount)?.try_into().map_err(invalid)?;
    let fee: u64 = chains::units(&t.fee)?.try_into().map_err(invalid)?;
    if amount < 546 || fee == 0 {
        return Err(Error::Invalid("amount or fee"));
    }
    let own_script = script(&tx.sender, network)?;
    let mut input = vec![];
    let mut previous = vec![];
    let mut total = 0u64;
    let mut unique = HashSet::new();
    for item in &t.utxos {
        let data = chains::hex_bytes(&item.transaction_hex)?;
        let prev: Transaction = consensus::deserialize(&data).map_err(invalid)?;
        let out = prev
            .output
            .get(item.vout as usize)
            .ok_or(Error::Invalid("previous output index"))?;
        if out.script_pubkey != own_script {
            return Err(Error::Invalid("input does not belong to sender"));
        }
        let point = OutPoint {
            txid: prev.compute_txid(),
            vout: item.vout,
        };
        if !unique.insert(point) {
            return Err(Error::Invalid("duplicate input"));
        }
        total = total
            .checked_add(out.value.to_sat())
            .ok_or(Error::Invalid("input amount overflow"))?;
        input.push(TxIn {
            previous_output: point,
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        });
        previous.push(prev);
    }
    let change = total
        .checked_sub(amount)
        .and_then(|v| v.checked_sub(fee))
        .ok_or(Error::Invalid("insufficient inputs"))?;
    if change > 0 && change < 546 {
        return Err(Error::Invalid("change below dust threshold"));
    }
    let mut output = vec![TxOut {
        value: Amount::from_sat(amount),
        script_pubkey: script(&t.to, network)?,
    }];
    if change > 0 {
        output.push(TxOut {
            value: Amount::from_sat(change),
            script_pubkey: own_script,
        });
    }
    let unsigned = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input,
        output,
    };
    let mut psbt = Psbt::from_unsigned_tx(unsigned).map_err(invalid)?;
    for (p, prev) in psbt.inputs.iter_mut().zip(previous) {
        p.non_witness_utxo = Some(prev);
        p.sighash_type = Some(bitcoin::psbt::PsbtSighashType::from_u32(
            if network == Network::BitcoinCash {
                0x41
            } else {
                1
            },
        ));
    }
    Ok(psbt)
}
fn digest(psbt: &Psbt, index: usize, network: Network) -> Result<[u8; 32]> {
    let tx = &psbt.unsigned_tx;
    let input = &tx.input[index];
    let prev = &psbt.inputs[index]
        .non_witness_utxo
        .as_ref()
        .ok_or(Error::Verification)?
        .output[input.previous_output.vout as usize];
    if network != Network::BitcoinCash {
        return SighashCache::new(tx)
            .legacy_signature_hash(index, &prev.script_pubkey, 1)
            .map(|h| h.to_byte_array())
            .map_err(invalid);
    }
    // BCH SIGHASH_ALL|FORKID is BIP143 serialization with the full 0x41 hash type.
    let points = tx
        .input
        .iter()
        .flat_map(|i| consensus::serialize(&i.previous_output))
        .collect::<Vec<_>>();
    let sequences = tx
        .input
        .iter()
        .flat_map(|i| i.sequence.to_consensus_u32().to_le_bytes())
        .collect::<Vec<_>>();
    let outputs = tx
        .output
        .iter()
        .flat_map(consensus::serialize)
        .collect::<Vec<_>>();
    let mut pre = consensus::serialize(&tx.version);
    pre.extend_from_slice(sha256d::Hash::hash(&points).as_byte_array());
    pre.extend_from_slice(sha256d::Hash::hash(&sequences).as_byte_array());
    pre.extend_from_slice(&consensus::serialize(&input.previous_output));
    pre.extend_from_slice(&consensus::serialize(&prev.script_pubkey));
    pre.extend_from_slice(&prev.value.to_sat().to_le_bytes());
    pre.extend_from_slice(&input.sequence.to_consensus_u32().to_le_bytes());
    pre.extend_from_slice(sha256d::Hash::hash(&outputs).as_byte_array());
    pre.extend_from_slice(&tx.lock_time.to_consensus_u32().to_le_bytes());
    pre.extend_from_slice(&0x41u32.to_le_bytes());
    Ok(sha256d::Hash::hash(&pre).to_byte_array())
}
pub fn inspect(t: &Transfer, tx: &Draft, network: Network) -> Result<Details> {
    let psbt = build(t, tx, network)?;
    let mut details = fields(
        &serde_json::json!({"Recipient":t.to,"Base units":t.amount,"Fee":t.fee,"Change address":tx.sender,"Change":psbt.unsigned_tx.output.get(1).map(|o|o.value.to_sat()).unwrap_or(0)}),
    )?;
    for (index, input) in psbt.unsigned_tx.input.iter().enumerate() {
        let prev = &psbt.inputs[index]
            .non_witness_utxo
            .as_ref()
            .ok_or(Error::Verification)?
            .output[input.previous_output.vout as usize];
        details.push((
            format!("Input {}", index + 1),
            format!(
                "{} · {} base units",
                input.previous_output,
                prev.value.to_sat()
            ),
        ));
    }
    Ok(Details {
        title: format!("{} transfer", network.chain().name),
        amount: format!(
            "{} {}",
            chains::display_units(chains::units(&t.amount)?, 8),
            network.chain().symbol
        ),
        fee: format!(
            "{} {}",
            chains::display_units(chains::units(&t.fee)?, 8),
            network.chain().symbol
        ),
        fields: details,
    })
}
pub fn sign(t: &Transfer, tx: &Draft, network: Network, key: &[u8]) -> Result<Vec<u8>> {
    let psbt = build(t, tx, network)?.serialize();
    let input = tw_proto::BitcoinV2::Proto::SigningInput {
        private_keys: vec![Cow::Borrowed(key)],
        transaction: tw_proto::BitcoinV2::Proto::mod_SigningInput::OneOftransaction::psbt(
            tw_proto::BitcoinV2::Proto::Psbt {
                psbt: Cow::Borrowed(&psbt),
            },
        ),
        ..Default::default()
    };
    let output = if network == Network::BitcoinCash {
        tw_bitcoincash::entry::BitcoinCashEntry.sign(&Context(network), input)
    } else {
        tw_bitcoin::entry::BitcoinEntry.sign(&Context(network), input)
    };
    if output.encoded.is_empty() {
        return Err(Error::Invalid("UTXO signing failed"));
    }
    Ok(output.encoded.into_owned())
}
pub fn verify(t: &Transfer, tx: &Draft, network: Network, raw: &[u8]) -> Result<String> {
    let expected = build(t, tx, network)?;
    let signed: Transaction = consensus::deserialize(raw).map_err(invalid)?;
    if signed.version != expected.unsigned_tx.version
        || signed.lock_time != expected.unsigned_tx.lock_time
        || signed.output != expected.unsigned_tx.output
        || signed.input.len() != expected.unsigned_tx.input.len()
        || consensus::serialize(&signed) != raw
    {
        return Err(Error::Verification);
    }
    let public = chains::hex_bytes(&tx.public_key)?;
    for (index, (actual, wanted)) in signed
        .input
        .iter()
        .zip(&expected.unsigned_tx.input)
        .enumerate()
    {
        if actual.previous_output != wanted.previous_output
            || actual.sequence != wanted.sequence
            || !actual.witness.is_empty()
        {
            return Err(Error::Verification);
        }
        let pushes = actual
            .script_sig
            .instructions_minimal()
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(invalid)?;
        if pushes.len() != 2 {
            return Err(Error::Verification);
        }
        let (signature, pubkey) = match (pushes[0], pushes[1]) {
            (
                bitcoin::script::Instruction::PushBytes(a),
                bitcoin::script::Instruction::PushBytes(b),
            ) => (a.as_bytes(), b.as_bytes()),
            _ => return Err(Error::Verification),
        };
        if signature.last()
            != Some(&if network == Network::BitcoinCash {
                0x41
            } else {
                1
            })
            || pubkey != public
        {
            return Err(Error::Verification);
        }
        let sig = bitcoin::secp256k1::ecdsa::Signature::from_der(&signature[..signature.len() - 1])
            .map_err(invalid)?;
        secp_verify(
            digest(&expected, index, network)?,
            &sig.serialize_compact(),
            &tx.public_key,
        )?;
        let expected_script = Builder::new()
            .push_slice(PushBytesBuf::try_from(signature.to_vec()).map_err(invalid)?)
            .push_slice(PushBytesBuf::try_from(public.clone()).map_err(invalid)?)
            .into_script();
        if actual.script_sig != expected_script
            || hash160::Hash::hash(pubkey).to_byte_array()
                != chains::bytes::<20>(
                    &keys::address_bytes(&tx.sender, network)?[if network == Network::BitcoinCash {
                        1
                    } else {
                        0
                    }..],
                )?
        {
            return Err(Error::Verification);
        }
    }
    Ok(signed.compute_txid().to_string())
}
