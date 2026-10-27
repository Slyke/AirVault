use super::{fields, Details, Transaction};
use crate::{
    chains::{self, bytes, invalid, keys},
    model::Network,
    Error, Result,
};
use blake2::{
    digest::{consts::U32, Digest},
    Blake2b,
};
use frame_metadata::{RuntimeMetadata, RuntimeMetadataPrefixed};
use parity_scale_codec::{Compact, Decode, Encode};
use scale_info::{PortableRegistry, TypeDef, TypeDefPrimitive};
use serde::{Deserialize, Serialize};

pub const GENESIS: &str = "68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Transfer {
    pub to: String,
    pub amount: String,
    pub nonce: u64,
    pub tip: String,
    pub spec_version: u32,
    pub transaction_version: u32,
    pub spec_name: String,
    pub metadata: String,
}
struct Prepared {
    call: Vec<u8>,
    extra: Vec<u8>,
    payload: Vec<u8>,
    metadata_hash: String,
}
fn array(registry: &PortableRegistry, id: u32, length: u32, depth: u8) -> bool {
    if depth > 8 {
        return false;
    }
    match registry.resolve(id).map(|t| &t.type_def) {
        Some(TypeDef::Composite(t)) if t.fields.len() == 1 => {
            array(registry, t.fields[0].ty.id, length, depth + 1)
        }
        Some(TypeDef::Array(t)) if t.len == length => matches!(
            registry.resolve(t.type_param.id).map(|t| &t.type_def),
            Some(TypeDef::Primitive(TypeDefPrimitive::U8))
        ),
        _ => false,
    }
}
fn prepare(t: &Transfer, network: Network) -> Result<Prepared> {
    let amount = chains::units(&t.amount)?;
    let tip = chains::units(&t.tip)?;
    if amount == 0
        || t.spec_version == 0
        || t.transaction_version == 0
        || !["statemint", "asset-hub-polkadot"].contains(&t.spec_name.as_str())
    {
        return Err(Error::Invalid("Polkadot runtime context"));
    }
    let raw = chains::hex_bytes(&t.metadata)?;
    let mut input = raw.as_slice();
    let metadata = RuntimeMetadataPrefixed::decode(&mut input).map_err(invalid)?;
    if metadata.0 != frame_metadata::META_RESERVED || !input.is_empty() {
        return Err(Error::Invalid("runtime metadata"));
    }
    let RuntimeMetadata::V15(ref meta) = metadata.1 else {
        return Err(Error::Invalid(
            "Polkadot requires version-15 runtime metadata",
        ));
    };
    if meta.extrinsic.version != 4 {
        return Err(Error::Invalid("runtime extrinsic version"));
    }
    let pallet = meta
        .pallets
        .iter()
        .find(|p| p.name == "Balances")
        .ok_or(Error::Invalid("Balances pallet missing"))?;
    let calls = meta
        .types
        .resolve(
            pallet
                .calls
                .as_ref()
                .ok_or(Error::Invalid("Balances calls"))?
                .ty
                .id,
        )
        .ok_or(Error::Invalid("call registry"))?;
    let TypeDef::Variant(variants) = &calls.type_def else {
        return Err(Error::Invalid("call variants"));
    };
    let call = variants
        .variants
        .iter()
        .find(|v| v.name == "transfer_keep_alive")
        .ok_or(Error::Invalid("transfer call missing"))?;
    if call.fields.len() != 2
        || call.fields[0].name.as_deref() != Some("dest")
        || call.fields[1].name.as_deref() != Some("value")
    {
        return Err(Error::Invalid("transfer arguments"));
    }
    let destination = meta
        .types
        .resolve(call.fields[0].ty.id)
        .ok_or(Error::Invalid("destination type"))?;
    let TypeDef::Variant(addresses) = &destination.type_def else {
        return Err(Error::Invalid("MultiAddress required"));
    };
    let id = addresses
        .variants
        .iter()
        .find(|v| v.name == "Id" && v.index == 0)
        .ok_or(Error::Invalid("AccountId address variant"))?;
    if id.fields.len() != 1 || !array(&meta.types, id.fields[0].ty.id, 32, 0) {
        return Err(Error::Invalid("AccountId32 required"));
    }
    let TypeDef::Compact(amount_type) = &meta
        .types
        .resolve(call.fields[1].ty.id)
        .ok_or(Error::Invalid("balance type"))?
        .type_def
    else {
        return Err(Error::Invalid("compact balance required"));
    };
    if !matches!(
        meta.types
            .resolve(amount_type.type_param.id)
            .map(|t| &t.type_def),
        Some(TypeDef::Primitive(TypeDefPrimitive::U128))
    ) {
        return Err(Error::Invalid("u128 balance required"));
    }
    let digest = merkleized_metadata::generate_metadata_digest(
        &metadata.1,
        merkleized_metadata::ExtraInfo {
            spec_version: t.spec_version,
            spec_name: t.spec_name.clone(),
            base58_prefix: 0,
            decimals: 10,
            token_symbol: "DOT".into(),
        },
    )
    .map_err(invalid)?
    .hash();
    let mut call_bytes = vec![pallet.index, call.index, 0];
    call_bytes.extend_from_slice(&keys::address_bytes(&t.to, network)?);
    call_bytes.extend(Compact(amount).encode());
    let mut extra = vec![];
    let mut additional = vec![];
    let mut found = false;
    let mut unique = std::collections::HashSet::new();
    for extension in &meta.extrinsic.signed_extensions {
        if !unique.insert(extension.identifier.as_str()) {
            return Err(Error::Invalid("duplicate signed extension"));
        }
        let extra_start = extra.len();
        let additional_start = additional.len();
        match extension.identifier.as_str() {
            "CheckNonZeroSender"
            | "CheckWeight"
            | "AuthorizeCall"
            | "StorageWeightReclaim"
            | "PrevalidateAttests"
            | "EthSetOrigin" => {}
            "CheckSpecVersion" => additional.extend(t.spec_version.to_le_bytes()),
            "CheckTxVersion" => additional.extend(t.transaction_version.to_le_bytes()),
            "CheckGenesis" => additional.extend(chains::hex_bytes(GENESIS)?),
            "CheckMortality" | "CheckEra" => {
                extra.push(0);
                additional.extend(chains::hex_bytes(GENESIS)?);
            }
            "CheckNonce" => extra.extend(Compact(t.nonce).encode()),
            "ChargeTransactionPayment" => extra.extend(Compact(tip).encode()),
            "ChargeAssetTxPayment" => {
                extra.extend(Compact(tip).encode());
                extra.push(0);
            }
            "CheckMetadataHash" => {
                extra.push(1);
                additional.push(1);
                additional.extend_from_slice(&digest);
                found = true;
            }
            _ => return Err(Error::Invalid("unknown runtime transaction extension")),
        }
        let mut encoded = &extra[extra_start..];
        scale_value::scale::decode_as_type(&mut encoded, extension.ty.id, &meta.types)
            .map_err(invalid)?;
        if !encoded.is_empty() {
            return Err(Error::Invalid("runtime extension encoding"));
        }
        let mut encoded = &additional[additional_start..];
        scale_value::scale::decode_as_type(
            &mut encoded,
            extension.additional_signed.id,
            &meta.types,
        )
        .map_err(invalid)?;
        if !encoded.is_empty() {
            return Err(Error::Invalid("runtime signed context encoding"));
        }
    }
    if !found
        || !unique.contains("CheckGenesis")
        || !unique.contains("CheckNonce")
        || !unique.contains("CheckSpecVersion")
        || !unique.contains("CheckTxVersion")
    {
        return Err(Error::Invalid(
            "required replay and metadata protections missing",
        ));
    }
    let mut payload = call_bytes.clone();
    payload.extend_from_slice(&extra);
    payload.extend(additional);
    if payload.len() > 256 {
        payload = Blake2b::<U32>::digest(payload).to_vec();
    }
    Ok(Prepared {
        call: call_bytes,
        extra,
        payload,
        metadata_hash: hex::encode(digest),
    })
}
pub fn inspect(t: &Transfer, _tx: &Transaction, network: Network) -> Result<Details> {
    let prepared = prepare(t, network)?;
    let fields = fields(
        &serde_json::json!({"Recipient":t.to,"Planck":t.amount,"Nonce":t.nonce,"Tip":t.tip,"Runtime":t.spec_name,"Spec version":t.spec_version,"Transaction version":t.transaction_version,"Metadata hash":prepared.metadata_hash,"Chain":"Polkadot Hub","Genesis":GENESIS,"Validity":"Until this nonce is used or the runtime changes","Call":"Balances.transfer_keep_alive"}),
    )?;
    Ok(Details {
        title: "Polkadot transfer".into(),
        amount: format!(
            "{} DOT",
            chains::display_units(chains::units(&t.amount)?, 10)
        ),
        fee: format!(
            "Network fee + {} DOT tip",
            chains::display_units(chains::units(&t.tip)?, 10)
        ),
        fields,
    })
}
fn encode(p: &Prepared, tx: &Transaction, sig: &[u8]) -> Result<Vec<u8>> {
    let mut body = vec![0x84, 0];
    body.extend(chains::hex_bytes(&tx.public_key)?);
    body.push(1);
    body.extend_from_slice(sig);
    body.extend_from_slice(&p.extra);
    body.extend_from_slice(&p.call);
    let mut result = Compact(body.len() as u32).encode();
    result.extend(body);
    Ok(result)
}
pub fn sign(t: &Transfer, tx: &Transaction, network: Network, key: &[u8]) -> Result<Vec<u8>> {
    let prepared = prepare(t, network)?;
    let pair = schnorrkel::MiniSecretKey::from_bytes(key)
        .map_err(invalid)?
        .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
    let signature = pair.sign(schnorrkel::signing_context(b"substrate").bytes(&prepared.payload));
    encode(&prepared, tx, &signature.to_bytes())
}
pub fn verify(t: &Transfer, tx: &Transaction, network: Network, raw: &[u8]) -> Result<String> {
    let prepared = prepare(t, network)?;
    let mut input = raw;
    let len = Compact::<u32>::decode(&mut input).map_err(invalid)?.0 as usize;
    if len != input.len() || input.len() < 99 || input[..2] != [0x84, 0] || input[34] != 1 {
        return Err(Error::Verification);
    }
    let public = chains::hex_bytes(&tx.public_key)?;
    if input[2..34] != public {
        return Err(Error::Verification);
    }
    let sig = schnorrkel::Signature::from_bytes(&input[35..99]).map_err(invalid)?;
    schnorrkel::PublicKey::from_bytes(&bytes::<32>(&public)?)
        .map_err(invalid)?
        .verify(
            schnorrkel::signing_context(b"substrate").bytes(&prepared.payload),
            &sig,
        )
        .map_err(|_| Error::Verification)?;
    if encode(&prepared, tx, &input[35..99])? != raw {
        return Err(Error::Verification);
    }
    Ok(format!("0x{}", hex::encode(Blake2b::<U32>::digest(raw))))
}
