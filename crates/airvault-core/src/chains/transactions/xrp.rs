use super::{fields, secp_verify, Details, Transaction};
use crate::{
    chains::{self, invalid, keys, Context},
    model::Network,
    Error, Result,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use std::borrow::Cow;
use tw_coin_entry::coin_entry::CoinEntry;
use tw_proto::Ripple::Proto as xrp;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Transfer {
    pub to: String,
    pub amount: String,
    pub fee: String,
    pub sequence: u32,
    pub last_ledger_sequence: u32,
    #[serde(default)]
    pub destination_tag: Option<u32>,
}
fn native_amount(value: &str) -> Result<u64> {
    let value = chains::units(value)?;
    if value == 0 || value > 100_000_000_000_000_000 {
        return Err(Error::Invalid("XRP amount range"));
    }
    Ok(value as u64)
}
fn parts(t: &Transfer, tx: &Transaction, network: Network) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut prefix = vec![0x12, 0, 0, 0x22];
    prefix.extend_from_slice(&0x80000000u32.to_be_bytes());
    prefix.push(0x24);
    prefix.extend_from_slice(&t.sequence.to_be_bytes());
    if let Some(tag) = t.destination_tag {
        prefix.push(0x2e);
        prefix.extend_from_slice(&tag.to_be_bytes());
    }
    prefix.extend_from_slice(&[0x20, 0x1b]);
    prefix.extend_from_slice(&t.last_ledger_sequence.to_be_bytes());
    prefix.push(0x61);
    prefix.extend_from_slice(&(native_amount(&t.amount)? | 0x4000000000000000).to_be_bytes());
    prefix.push(0x68);
    prefix.extend_from_slice(&(native_amount(&t.fee)? | 0x4000000000000000).to_be_bytes());
    let public = chains::hex_bytes(&tx.public_key)?;
    if public.len() != 33 {
        return Err(Error::Invalid("XRP public key"));
    }
    prefix.extend_from_slice(&[0x73, 33]);
    prefix.extend_from_slice(&public);
    let mut suffix = vec![0x81, 20];
    suffix.extend_from_slice(&keys::address_bytes(&tx.sender, network)?);
    suffix.extend_from_slice(&[0x83, 20]);
    suffix.extend_from_slice(&keys::address_bytes(&t.to, network)?);
    Ok((prefix, suffix))
}
pub fn inspect(t: &Transfer, tx: &Transaction, network: Network) -> Result<Details> {
    parts(t, tx, network)?;
    if t.sequence == 0 || t.last_ledger_sequence == 0 || native_amount(&t.fee)? > 1_000_000 {
        return Err(Error::Invalid("XRP sequence, expiration or fee"));
    }
    Ok(Details {
        title: "XRP payment".into(),
        amount: format!(
            "{} XRP",
            chains::display_units(chains::units(&t.amount)?, 6)
        ),
        fee: format!("{} XRP", chains::display_units(chains::units(&t.fee)?, 6)),
        fields: fields(t)?,
    })
}
pub fn sign(t: &Transfer, tx: &Transaction, network: Network, key: &[u8]) -> Result<Vec<u8>> {
    inspect(t, tx, network)?;
    let mut json = serde_json::json!({"TransactionType":"Payment","Account":tx.sender,"Destination":t.to,"Amount":t.amount,"Fee":t.fee,"Sequence":t.sequence,"LastLedgerSequence":t.last_ledger_sequence,"Flags":2147483648u32});
    if let Some(tag) = t.destination_tag {
        json["DestinationTag"] = tag.into();
    }
    let input = xrp::SigningInput {
        private_key: Cow::Borrowed(key),
        raw_json: Cow::Owned(json.to_string()),
        ..Default::default()
    };
    let output = tw_ripple::entry::RippleEntry.sign(&Context(network), input);
    if output.error != tw_proto::Common::Proto::SigningError::OK {
        return Err(Error::Invalid("XRP signing failed"));
    }
    Ok(output.encoded.into_owned())
}
pub fn verify(t: &Transfer, tx: &Transaction, network: Network, raw: &[u8]) -> Result<String> {
    inspect(t, tx, network)?;
    let (prefix, suffix) = parts(t, tx, network)?;
    if raw.len() < prefix.len() + suffix.len() + 10
        || !raw.starts_with(&prefix)
        || !raw.ends_with(&suffix)
        || raw[prefix.len()] != 0x74
    {
        return Err(Error::Verification);
    }
    let len = raw[prefix.len() + 1] as usize;
    if !(8..=72).contains(&len) || raw.len() != prefix.len() + 2 + len + suffix.len() {
        return Err(Error::Verification);
    }
    let sig = bitcoin::secp256k1::ecdsa::Signature::from_der(
        &raw[prefix.len() + 2..prefix.len() + 2 + len],
    )
    .map_err(invalid)?;
    if sig.serialize_der().as_ref() != &raw[prefix.len() + 2..prefix.len() + 2 + len] {
        return Err(Error::Verification);
    }
    let mut signing = vec![0x53, 0x54, 0x58, 0];
    signing.extend_from_slice(&prefix);
    signing.extend_from_slice(&suffix);
    secp_verify(
        chains::bytes(&Sha512::digest(signing)[..32])?,
        &sig.serialize_compact(),
        &tx.public_key,
    )?;
    let mut hash = Sha512::new();
    hash.update([0x54, 0x58, 0x4e, 0]);
    hash.update(raw);
    Ok(hex::encode_upper(&hash.finalize()[..32]))
}
