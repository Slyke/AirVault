use super::*;
use bitcoin::consensus;
use blake2::{
    digest::{consts::U16, Digest},
    Blake2b,
};
use parity_scale_codec::Decode;
use scale_value::At;
use std::hash::Hasher;

fn metadata(mode: Mode, base: &str) -> Result<Vec<u8>> {
    let result = rpc(
        mode,
        base,
        "state_call",
        json!(["Metadata_metadata_at_version", "0x0f000000"]),
    )?;
    let encoded = chains::hex_bytes(text(&result)?)?;
    let mut input = encoded.as_slice();
    let data = Option::<Vec<u8>>::decode(&mut input)
        .map_err(invalid)?
        .ok_or(Error::Invalid("version-15 metadata unavailable"))?;
    if !input.is_empty() || data.len() > 2 * 1024 * 1024 {
        return Err(Error::Invalid("runtime metadata size"));
    }
    Ok(data)
}
fn twox128(data: &[u8]) -> Vec<u8> {
    let mut result = vec![];
    for seed in [0, 1] {
        let mut h = twox_hash::XxHash64::with_seed(seed);
        h.write(data);
        result.extend(h.finish().to_le_bytes());
    }
    result
}
pub(super) fn polkadot_balance(mode: Mode, base: &str, address: &str) -> Result<u128> {
    let public = keys::address_bytes(address, Network::Polkadot)?;
    let mut key = twox128(b"System");
    key.extend(twox128(b"Account"));
    key.extend(Blake2b::<U16>::digest(&public));
    key.extend(public);
    let value = rpc(
        mode,
        base,
        "state_getStorage",
        json!([format!("0x{}", hex::encode(key))]),
    )?;
    if value.is_null() {
        return Ok(0);
    }
    let encoded = chains::hex_bytes(text(&value)?)?;
    let raw = metadata(mode, base)?;
    let frame_metadata::RuntimeMetadata::V15(meta) =
        frame_metadata::RuntimeMetadataPrefixed::decode(&mut raw.as_slice())
            .map_err(invalid)?
            .1
    else {
        return Err(Error::Invalid("runtime metadata version"));
    };
    let system = meta
        .pallets
        .iter()
        .find(|p| p.name == "System")
        .ok_or(Error::Network)?;
    let account = system
        .storage
        .as_ref()
        .and_then(|s| s.entries.iter().find(|e| e.name == "Account"))
        .ok_or(Error::Network)?;
    let frame_metadata::v15::StorageEntryType::Map { value, .. } = &account.ty else {
        return Err(Error::Network);
    };
    let mut input = encoded.as_slice();
    let decoded =
        scale_value::scale::decode_as_type(&mut input, value.id, &meta.types).map_err(invalid)?;
    if !input.is_empty() {
        return Err(Error::Network);
    }
    decoded
        .at("data")
        .at("free")
        .and_then(|v| v.as_u128())
        .ok_or(Error::Network)
}
pub fn fetch_state(settings: &Settings, account: &Account) -> Result<Value> {
    let base = provider(settings, account.network)?;
    let mode = settings.mode;
    let n = account.network;
    let address = &account.address;
    verify_chain(mode, &base, n)?;
    if n.is_bitcoin() {
        return Ok(json!({"utxos":network::utxos(mode,&base,address,n)?}));
    }
    if n.evm_id().is_some() {
        return Ok(
            json!({"nonce":hex_number(&rpc(mode,&base,"eth_getTransactionCount",json!([address,"pending"]))?)?.to_string(),"gasPrice":hex_number(&rpc(mode,&base,"eth_gasPrice",json!([]))?)?.to_string()}),
        );
    }
    match n {
        Network::Solana | Network::SolanaDevnet => {
            let block = rpc(
                mode,
                &base,
                "getLatestBlockhash",
                json!([{"commitment":"finalized"}]),
            )?;
            Ok(json!({"recentBlockhash":text(&block["value"]["blockhash"])?}))
        }
        Network::Cosmos => {
            let data = get(
                mode,
                &base,
                &format!("cosmos/auth/v1beta1/accounts/{address}"),
            )?;
            let mut account = &data["account"];
            if account.get("base_account").is_some() {
                account = &account["base_account"];
            }
            if account.get("base_vesting_account").is_some() {
                account = &account["base_vesting_account"]["base_account"];
            }
            Ok(
                json!({"accountNumber":integer(&account["account_number"])?,"sequence":integer(&account["sequence"])?}),
            )
        }
        Network::Xrp => {
            let account = rpc(
                mode,
                &base,
                "account_info",
                json!([{"account":address,"ledger_index":"validated"}]),
            )?;
            let fees = rpc(mode, &base, "fee", json!([{}]))?;
            let ledger = integer(&fees["ledger_current_index"])?;
            Ok(
                json!({"sequence":integer(&account["account_data"]["Sequence"])?,"lastLedgerSequence":ledger.checked_add(20).ok_or(Error::Network)?,"fee":text(&fees["drops"]["open_ledger_fee"])?}),
            )
        }
        Network::Tron | Network::TronNile => {
            let block = post(mode, &base, "wallet/getnowblock", json!({}))?;
            let number = integer(&block["block_header"]["raw_data"]["number"])?;
            let timestamp = integer(&block["block_header"]["raw_data"]["timestamp"])?;
            let hash = chains::hex_bytes(text(&block["blockID"])?)?;
            let hash: [u8; 32] = chains::bytes(&hash)?;
            Ok(
                json!({"refBlockBytes":hex::encode(&number.to_be_bytes()[6..]),"refBlockHash":hex::encode(&hash[8..16]),"timestamp":timestamp,"expiration":timestamp.checked_add(600000).ok_or(Error::Network)?}),
            )
        }
        Network::Polkadot => {
            let version = rpc(mode, &base, "state_getRuntimeVersion", json!([]))?;
            let nonce = rpc(mode, &base, "system_accountNextIndex", json!([address]))?;
            Ok(
                json!({"nonce":integer(&nonce)?,"specVersion":integer(&version["specVersion"])?,"transactionVersion":integer(&version["transactionVersion"])?,"specName":text(&version["specName"])?,"metadata":hex::encode(metadata(mode,&base)?)}),
            )
        }
        Network::Dogecoin | Network::Litecoin | Network::BitcoinCash => {
            let list = get(mode, &base, &format!("utxo/{address}"))?;
            let list = list.as_array().ok_or(Error::Network)?;
            if list.len() > 100 {
                return Err(Error::Invalid("select at most 100 inputs"));
            }
            let mut utxos = vec![];
            for item in list {
                let id = text(&item["txid"])?;
                let expected = id.parse::<bitcoin::Txid>().map_err(invalid)?;
                let previous = get(mode, &base, &format!("tx-specific/{id}"))?;
                let data = text(&previous["hex"])?;
                let transaction: bitcoin::Transaction =
                    consensus::deserialize(&chains::hex_bytes(data)?).map_err(invalid)?;
                let vout: u32 = integer(&item["vout"])?.try_into().map_err(invalid)?;
                if transaction.compute_txid() != expected
                    || transaction.output.get(vout as usize).is_none_or(|o| {
                        o.script_pubkey
                            != chains::transactions::utxo::script(address, n).unwrap_or_default()
                    })
                {
                    return Err(Error::Verification);
                }
                utxos.push(json!({"transactionHex":data,"vout":vout}));
            }
            Ok(json!({"utxos":utxos}))
        }
        Network::Cardano | Network::CardanoPreprod => {
            let tip = get(mode, &base, "tip")?;
            let slot = integer(&tip[0]["abs_slot"])?;
            let info = post(mode, &base, "address_info", json!({"_addresses":[address]}))?;
            let list = info[0]["utxo_set"].as_array().ok_or(Error::Network)?;
            if list.len() > 100 {
                return Err(Error::Invalid("select at most 100 inputs"));
            }
            let mut utxos = vec![];
            for item in list {
                if item["asset_list"].as_array().is_some_and(|v| !v.is_empty()) {
                    continue;
                }
                let id = text(&item["tx_hash"])?;
                let _: [u8; 32] = chains::bytes(&chains::hex_bytes(id)?)?;
                let previous = post(mode, &base, "tx_cbor", json!({"_tx_hashes":[id]}))?;
                let cbor = text(&previous[0]["cbor"])?;
                let tx = cardano_serialization_lib::FixedTransaction::from_bytes(
                    chains::hex_bytes(cbor)?,
                )
                .map_err(invalid)?;
                if hex::encode(tx.transaction_hash().to_bytes()) != id {
                    return Err(Error::Verification);
                }
                let index = integer(&item["tx_index"])?;
                if index as usize >= tx.body().outputs().len()
                    || tx
                        .body()
                        .outputs()
                        .get(index as usize)
                        .address()
                        .to_bech32(None)
                        .map_err(invalid)?
                        != *address
                {
                    return Err(Error::Verification);
                }
                utxos.push(json!({"transactionCbor":cbor,"index":index}));
            }
            Ok(json!({"invalidAfter":slot.checked_add(3600).ok_or(Error::Network)?,"utxos":utxos}))
        }
        _ => Err(Error::Invalid("provider network")),
    }
}
