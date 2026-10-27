use super::{fields, secp_verify, Details, Transaction};
use crate::{
    chains::{self, bytes, invalid, keys},
    model::Network,
    Error, Result,
};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tw_keypair::traits::SigningKeyTrait;
use tw_misc::traits::ToBytesVec;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Transfer {
    pub to: String,
    pub amount: String,
    pub ref_block_bytes: String,
    pub ref_block_hash: String,
    pub timestamp: i64,
    pub expiration: i64,
    #[serde(default)]
    pub fee_limit: i64,
    #[serde(default)]
    pub asset: Option<Token>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Token {
    pub contract: String,
    pub symbol: String,
    pub decimals: u8,
}
#[derive(Clone, PartialEq, Message)]
struct Any {
    #[prost(string, tag = "1")]
    url: String,
    #[prost(bytes = "vec", tag = "2")]
    value: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct Contract {
    #[prost(int32, tag = "1")]
    kind: i32,
    #[prost(message, optional, tag = "2")]
    parameter: Option<Any>,
}
#[derive(Clone, PartialEq, Message)]
struct Send {
    #[prost(bytes = "vec", tag = "1")]
    owner: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    to: Vec<u8>,
    #[prost(int64, tag = "3")]
    amount: i64,
}
#[derive(Clone, PartialEq, Message)]
struct Trigger {
    #[prost(bytes = "vec", tag = "1")]
    owner: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    contract: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    data: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct Raw {
    #[prost(bytes = "vec", tag = "1")]
    ref_bytes: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    ref_hash: Vec<u8>,
    #[prost(int64, tag = "8")]
    expiration: i64,
    #[prost(message, repeated, tag = "11")]
    contracts: Vec<Contract>,
    #[prost(int64, tag = "14")]
    timestamp: i64,
    #[prost(int64, tag = "18")]
    fee_limit: i64,
}
#[derive(Clone, PartialEq, Message)]
struct Signed {
    #[prost(message, optional, tag = "1")]
    raw: Option<Raw>,
    #[prost(bytes = "vec", repeated, tag = "2")]
    signatures: Vec<Vec<u8>>,
}
fn raw(t: &Transfer, tx: &Transaction, network: Network) -> Result<Raw> {
    let ref_bytes = chains::hex_bytes(&t.ref_block_bytes)?;
    let ref_hash = chains::hex_bytes(&t.ref_block_hash)?;
    if ref_bytes.len() != 2
        || ref_hash.len() != 8
        || t.timestamp <= 0
        || t.expiration <= t.timestamp
        || t.expiration - t.timestamp > 86_400_000
        || !(0..=15_000_000_000).contains(&t.fee_limit)
    {
        return Err(Error::Invalid(
            "Tron reference block, lifetime or fee limit",
        ));
    }
    let owner = keys::address_bytes(&tx.sender, network)?;
    let to = keys::address_bytes(&t.to, network)?;
    let amount = chains::units(&t.amount)?;
    if amount == 0 {
        return Err(Error::Invalid("amount must be greater than zero"));
    }
    let (kind, url, value) = if let Some(asset) = &t.asset {
        if asset.decimals > 36
            || asset.symbol.is_empty()
            || asset.symbol.len() > 16
            || !asset.symbol.bytes().all(|b| b.is_ascii_alphanumeric())
            || t.fee_limit == 0
        {
            return Err(Error::Invalid("TRC-20 token metadata or fee limit"));
        }
        let mut data = vec![0xa9, 0x05, 0x9c, 0xbb];
        data.extend_from_slice(&[0; 12]);
        data.extend_from_slice(&to[1..]);
        data.extend_from_slice(&[0; 16]);
        data.extend_from_slice(&amount.to_be_bytes());
        (
            31,
            "TriggerSmartContract",
            Trigger {
                owner,
                contract: keys::address_bytes(&asset.contract, network)?,
                data,
            }
            .encode_to_vec(),
        )
    } else {
        if t.fee_limit != 0 {
            return Err(Error::Invalid(
                "native TRX transfer has no energy fee limit",
            ));
        }
        (
            1,
            "TransferContract",
            Send {
                owner,
                to,
                amount: amount.try_into().map_err(invalid)?,
            }
            .encode_to_vec(),
        )
    };
    Ok(Raw {
        ref_bytes,
        ref_hash,
        expiration: t.expiration,
        timestamp: t.timestamp,
        fee_limit: t.fee_limit,
        contracts: vec![Contract {
            kind,
            parameter: Some(Any {
                url: format!("type.googleapis.com/protocol.{url}"),
                value,
            }),
        }],
    })
}
pub fn inspect(t: &Transfer, tx: &Transaction, network: Network) -> Result<Details> {
    raw(t, tx, network)?;
    let (symbol, decimals) = t
        .asset
        .as_ref()
        .map(|a| (a.symbol.as_str(), a.decimals))
        .unwrap_or(("TRX", 6));
    Ok(Details {
        title: "Tron transfer".into(),
        amount: format!(
            "{} {symbol}",
            chains::display_units(chains::units(&t.amount)?, decimals)
        ),
        fee: if t.asset.is_some() {
            format!(
                "Energy limit {} TRX + network bandwidth/account fees",
                chains::display_units(t.fee_limit as u128, 6)
            )
        } else {
            "Network bandwidth and account-creation fees".into()
        },
        fields: fields(t)?,
    })
}
pub fn sign(t: &Transfer, tx: &Transaction, network: Network, key: &[u8]) -> Result<Vec<u8>> {
    let raw = raw(t, tx, network)?;
    let hash = Sha256::digest(raw.encode_to_vec());
    let signature = tw_keypair::ecdsa::secp256k1::PrivateKey::try_from(key)
        .map_err(invalid)?
        .sign(tw_hash::H256::try_from(hash.as_slice()).map_err(invalid)?)
        .map_err(invalid)?
        .to_vec();
    Ok(Signed {
        raw: Some(raw),
        signatures: vec![signature],
    }
    .encode_to_vec())
}
pub fn verify(t: &Transfer, tx: &Transaction, network: Network, data: &[u8]) -> Result<String> {
    let decoded = Signed::decode(data).map_err(invalid)?;
    let expected = raw(t, tx, network)?;
    if decoded.raw.as_ref() != Some(&expected)
        || decoded.signatures.len() != 1
        || decoded.encode_to_vec() != data
    {
        return Err(Error::Verification);
    }
    let signature = &decoded.signatures[0];
    if signature.len() != 65 || signature[64] > 1 {
        return Err(Error::Verification);
    }
    let digest: [u8; 32] = Sha256::digest(expected.encode_to_vec()).into();
    secp_verify(digest, &signature[..64], &tx.public_key)?;
    use bitcoin::secp256k1::{
        ecdsa::{RecoverableSignature, RecoveryId},
        Message as SecpMessage, Secp256k1,
    };
    let sig = RecoverableSignature::from_compact(
        &signature[..64],
        RecoveryId::from_i32(signature[64] as i32).map_err(invalid)?,
    )
    .map_err(invalid)?;
    let public = Secp256k1::verification_only()
        .recover_ecdsa(&SecpMessage::from_digest(bytes(&digest)?), &sig)
        .map_err(invalid)?;
    if hex::encode(public.serialize()) != tx.public_key {
        return Err(Error::Verification);
    }
    Ok(hex::encode(digest))
}
