use super::{fields, Details, Transaction as Draft};
use crate::{
    chains::{self, bytes, invalid, keys},
    model::Network,
    Error, Result,
};
use blake2::{
    digest::{consts::U32, Digest},
    Blake2b,
};
use cardano_serialization_lib as ada;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Utxo {
    pub transaction_cbor: String,
    pub index: u32,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Transfer {
    pub to: String,
    pub amount: String,
    pub fee: String,
    pub invalid_after: u64,
    pub utxos: Vec<Utxo>,
}
fn number(value: &str) -> Result<ada::BigNum> {
    let _: u64 = chains::units(value)?.try_into().map_err(invalid)?;
    ada::BigNum::from_str(value).map_err(invalid)
}
fn body(t: &Transfer, tx: &Draft, network: Network) -> Result<ada::TransactionBody> {
    keys::address_bytes(&t.to, network)?;
    if t.utxos.is_empty() || t.utxos.len() > 100 || t.invalid_after == 0 {
        return Err(Error::Invalid("Cardano inputs or expiration"));
    }
    let amount: u64 = chains::units(&t.amount)?.try_into().map_err(invalid)?;
    let fee: u64 = chains::units(&t.fee)?.try_into().map_err(invalid)?;
    if amount < 1_000_000 || fee == 0 {
        return Err(Error::Invalid("ADA output minimum or fee"));
    }
    let mut inputs = ada::TransactionInputs::new();
    let mut total = 0u64;
    let mut unique = HashSet::new();
    for item in &t.utxos {
        let previous =
            ada::FixedTransaction::from_bytes(chains::hex_bytes(&item.transaction_cbor)?)
                .map_err(invalid)?;
        let outputs = previous.body().outputs();
        if item.index as usize >= outputs.len() {
            return Err(Error::Invalid("Cardano output index"));
        }
        let output = outputs.get(item.index as usize);
        if output.address().to_bech32(None).map_err(invalid)? != tx.sender
            || output.amount().multiasset().is_some_and(|m| m.len() > 0)
            || output.plutus_data().is_some()
            || output.script_ref().is_some()
        {
            return Err(Error::Invalid("use ADA-only inputs owned by the sender"));
        }
        let hash = previous.transaction_hash();
        if !unique.insert((hash.to_bytes(), item.index)) {
            return Err(Error::Invalid("duplicate Cardano input"));
        }
        inputs.add(&ada::TransactionInput::new(&hash, item.index));
        total = total
            .checked_add(
                output
                    .amount()
                    .coin()
                    .to_str()
                    .parse::<u64>()
                    .map_err(invalid)?,
            )
            .ok_or(Error::Invalid("input overflow"))?;
    }
    let change = total
        .checked_sub(amount)
        .and_then(|v| v.checked_sub(fee))
        .ok_or(Error::Invalid("insufficient ADA"))?;
    if change > 0 && change < 1_000_000 {
        return Err(Error::Invalid("ADA change below minimum"));
    }
    let mut outputs = ada::TransactionOutputs::new();
    outputs.add(&ada::TransactionOutput::new(
        &ada::Address::from_bech32(&t.to).map_err(invalid)?,
        &ada::Value::new(&number(&t.amount)?),
    ));
    if change > 0 {
        outputs.add(&ada::TransactionOutput::new(
            &ada::Address::from_bech32(&tx.sender).map_err(invalid)?,
            &ada::Value::new(&number(&change.to_string())?),
        ));
    }
    let mut result = ada::TransactionBody::new_tx_body(&inputs, &outputs, &number(&t.fee)?);
    result.set_ttl(&number(&t.invalid_after.to_string())?);
    Ok(result)
}
pub fn inspect(t: &Transfer, tx: &Draft, network: Network) -> Result<Details> {
    let body = body(t, tx, network)?;
    let outputs = body.outputs();
    let mut details = fields(
        &serde_json::json!({"Recipient":t.to,"Lovelace":t.amount,"Fee":t.fee,"Expires at slot":t.invalid_after,"Change address":tx.sender,"Change lovelace":if outputs.len()>1{outputs.get(1).amount().coin().to_str()}else{"0".into()}}),
    )?;
    for i in 0..body.inputs().len() {
        let input = body.inputs().get(i);
        details.push((
            format!("Input {}", i + 1),
            format!(
                "{}#{}",
                hex::encode(input.transaction_id().to_bytes()),
                input.index()
            ),
        ));
    }
    Ok(Details {
        title: "Cardano transfer".into(),
        amount: format!(
            "{} ADA",
            chains::display_units(chains::units(&t.amount)?, 6)
        ),
        fee: format!("{} ADA", chains::display_units(chains::units(&t.fee)?, 6)),
        fields: details,
    })
}
pub fn sign(t: &Transfer, tx: &Draft, network: Network, key: &[u8]) -> Result<Vec<u8>> {
    let body = body(t, tx, network)?;
    if key.len() != 128 {
        return Err(Error::Invalid("Cardano signing key"));
    }
    let digest = Blake2b::<U32>::digest(body.to_bytes());
    let private = ada::Bip32PrivateKey::from_bytes(&key[..96])
        .map_err(invalid)?
        .to_raw_key();
    let witness = ada::make_vkey_witness(
        &ada::TransactionHash::from_bytes(digest.to_vec()).map_err(invalid)?,
        &private,
    );
    let mut keys = ada::Vkeywitnesses::new();
    keys.add(&witness);
    let mut set = ada::TransactionWitnessSet::new();
    set.set_vkeys(&keys);
    Ok(ada::Transaction::new(&body, &set, None).to_bytes())
}
pub fn verify(t: &Transfer, tx: &Draft, network: Network, raw: &[u8]) -> Result<String> {
    let expected = body(t, tx, network)?;
    let actual = ada::FixedTransaction::from_bytes(raw.to_vec()).map_err(invalid)?;
    if actual.raw_body() != expected.to_bytes() {
        return Err(Error::Verification);
    }
    let full = ada::Transaction::from_bytes(raw.to_vec()).map_err(invalid)?;
    let witnesses = full.witness_set().vkeys().ok_or(Error::Verification)?;
    if witnesses.len() != 1 {
        return Err(Error::Verification);
    }
    let witness = witnesses.get(0);
    let public = witness.vkey().public_key().as_bytes();
    let expected_public = chains::hex_bytes(&tx.public_key)?;
    if expected_public.len() != 64 || public != expected_public[..32] {
        return Err(Error::Verification);
    }
    let digest = Blake2b::<U32>::digest(expected.to_bytes());
    let public = ed25519_dalek::VerifyingKey::from_bytes(&bytes(&public)?).map_err(invalid)?;
    let signature =
        ed25519_dalek::Signature::from_slice(&witness.signature().to_bytes()).map_err(invalid)?;
    public
        .verify_strict(&digest, &signature)
        .map_err(|_| Error::Verification)?;
    let mut only_vkeys = ada::TransactionWitnessSet::new();
    only_vkeys.set_vkeys(&witnesses);
    if ada::Transaction::new(&expected, &only_vkeys, None).to_bytes() != raw {
        return Err(Error::Verification);
    }
    Ok(hex::encode(digest))
}
