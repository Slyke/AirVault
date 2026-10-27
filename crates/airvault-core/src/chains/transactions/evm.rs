use super::{fields, secp_verify, Details, Transaction};
use crate::{
    chains::{self, invalid, keys, Context},
    model::Network,
    Error, Result,
};
use rlp::{Rlp, RlpStream};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::borrow::Cow;
use tw_coin_entry::coin_entry::CoinEntry;
use tw_proto::Ethereum::Proto as eth;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Transfer {
    pub to: String,
    pub amount: String,
    pub nonce: u64,
    pub gas_limit: u64,
    pub gas_price: String,
    #[serde(default)]
    pub asset: Option<Asset>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "standard", rename_all = "snake_case", deny_unknown_fields)]
pub enum Asset {
    Erc20 {
        contract: String,
        symbol: String,
        decimals: u8,
    },
    Erc721 {
        contract: String,
        token_id: String,
    },
    Erc1155 {
        contract: String,
        token_id: String,
    },
}
fn word(value: u128) -> [u8; 32] {
    let mut out = [0; 32];
    out[16..].copy_from_slice(&value.to_be_bytes());
    out
}
fn address_word(address: &str) -> Result<[u8; 32]> {
    let mut out = [0; 32];
    out[12..].copy_from_slice(&keys::evm_bytes(address)?);
    Ok(out)
}
fn minimal(value: u128) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    bytes[bytes.iter().position(|b| *b != 0).unwrap_or(16)..].to_vec()
}
fn call(t: &Transfer, tx: &Transaction) -> Result<([u8; 20], u128, Vec<u8>)> {
    let to = keys::evm_bytes(&t.to)?;
    let amount = chains::units(&t.amount)?;
    if amount == 0 {
        return Err(Error::Invalid("amount must be greater than zero"));
    }
    let mut data = vec![];
    let Some(asset) = &t.asset else {
        return Ok((to, amount, data));
    };
    let contract = match asset {
        Asset::Erc20 {
            contract,
            symbol,
            decimals,
        } => {
            if symbol.is_empty()
                || symbol.len() > 16
                || !symbol.bytes().all(|b| b.is_ascii_alphanumeric())
                || *decimals > 36
            {
                return Err(Error::Invalid("token metadata"));
            }
            data.extend_from_slice(&[0xa9, 0x05, 0x9c, 0xbb]);
            data.extend_from_slice(&address_word(&t.to)?);
            data.extend_from_slice(&word(amount));
            contract
        }
        Asset::Erc721 { contract, token_id } => {
            if amount != 1 {
                return Err(Error::Invalid("ERC-721 amount must be one"));
            }
            data.extend_from_slice(&[0x42, 0x84, 0x2e, 0x0e]);
            data.extend_from_slice(&address_word(&tx.sender)?);
            data.extend_from_slice(&address_word(&t.to)?);
            data.extend_from_slice(&word(chains::units(token_id)?));
            contract
        }
        Asset::Erc1155 { contract, token_id } => {
            data.extend_from_slice(&[0xf2, 0x42, 0x43, 0x2a]);
            data.extend_from_slice(&address_word(&tx.sender)?);
            data.extend_from_slice(&address_word(&t.to)?);
            data.extend_from_slice(&word(chains::units(token_id)?));
            data.extend_from_slice(&word(amount));
            data.extend_from_slice(&word(160));
            data.extend_from_slice(&word(0));
            contract
        }
    };
    Ok((keys::evm_bytes(contract)?, 0, data))
}
fn unsigned_fields(t: &Transfer, tx: &Transaction) -> Result<Vec<Vec<u8>>> {
    let (to, value, data) = call(t, tx)?;
    Ok(vec![
        minimal(t.nonce as u128),
        minimal(chains::units(&t.gas_price)?),
        minimal(t.gas_limit as u128),
        to.to_vec(),
        minimal(value),
        data,
    ])
}
fn encode(fields: &[Vec<u8>]) -> Vec<u8> {
    let mut rlp = RlpStream::new_list(fields.len());
    for value in fields {
        rlp.append(&value.as_slice());
    }
    rlp.out().to_vec()
}
pub fn inspect(t: &Transfer, tx: &Transaction, network: Network) -> Result<Details> {
    let id = network.evm_id().ok_or(Error::Invalid("EVM chain"))?;
    call(t, tx)?;
    if !(21000..=30_000_000).contains(&t.gas_limit) {
        return Err(Error::Invalid("gas limit"));
    }
    let gas_price = chains::units(&t.gas_price)?;
    if gas_price == 0 {
        return Err(Error::Invalid("gas price"));
    }
    let fee = gas_price
        .checked_mul(t.gas_limit as u128)
        .ok_or(Error::Invalid("fee overflow"))?;
    let (symbol, decimals) = match &t.asset {
        Some(Asset::Erc20 {
            symbol, decimals, ..
        }) => (symbol.as_str(), *decimals),
        Some(_) => ("NFT units", 0),
        None => (network.chain().symbol, 18),
    };
    let mut fields = fields(t)?;
    fields.insert(0, ("Chain ID".into(), id.to_string()));
    Ok(Details {
        title: format!("{} transfer", network.chain().name),
        amount: format!(
            "{} {symbol}",
            chains::display_units(chains::units(&t.amount)?, decimals)
        ),
        fee: format!(
            "Execution cap: {} {}{}",
            chains::display_units(fee, 18),
            network.chain().symbol,
            if matches!(network, Network::Optimism | Network::Base) {
                " + L1 data fee"
            } else {
                ""
            }
        ),
        fields,
    })
}
pub fn sign(t: &Transfer, tx: &Transaction, network: Network, key: &[u8]) -> Result<Vec<u8>> {
    inspect(t, tx, network)?;
    let (to, value, data) = call(t, tx)?;
    let input = eth::SigningInput {
        chain_id: Cow::Owned(minimal(
            network.evm_id().ok_or(Error::Invalid("EVM chain"))? as u128,
        )),
        nonce: Cow::Owned(minimal(t.nonce as u128)),
        gas_price: Cow::Owned(minimal(chains::units(&t.gas_price)?)),
        gas_limit: Cow::Owned(minimal(t.gas_limit as u128)),
        to_address: Cow::Owned(format!("0x{}", hex::encode(to))),
        private_key: Cow::Borrowed(key),
        transaction: Some(eth::Transaction {
            transaction_oneof: eth::mod_Transaction::OneOftransaction_oneof::contract_generic(
                eth::mod_Transaction::ContractGeneric {
                    amount: Cow::Owned(minimal(value)),
                    data: Cow::Owned(data),
                },
            ),
        }),
        ..Default::default()
    };
    let result = tw_ethereum::entry::EthereumEntry.sign(&Context(network), input);
    if result.error != tw_proto::Common::Proto::SigningError::OK {
        return Err(Error::Invalid("EVM signing failed"));
    }
    Ok(result.encoded.into_owned())
}
pub fn verify(t: &Transfer, tx: &Transaction, network: Network, raw: &[u8]) -> Result<String> {
    inspect(t, tx, network)?;
    let rlp = Rlp::new(raw);
    if rlp.item_count().map_err(invalid)? != 9 || rlp.as_raw().len() != raw.len() {
        return Err(Error::Verification);
    }
    let fields = unsigned_fields(t, tx)?;
    for (index, expected) in fields.iter().enumerate() {
        if rlp.at(index).map_err(invalid)?.data().map_err(invalid)? != expected {
            return Err(Error::Verification);
        }
    }
    let v: u64 = rlp.val_at(6).map_err(invalid)?;
    let base = network
        .evm_id()
        .ok_or(Error::Verification)?
        .checked_mul(2)
        .and_then(|n| n.checked_add(35))
        .ok_or(Error::Verification)?;
    if v != base && v != base + 1 {
        return Err(Error::Verification);
    }
    let mut unsigned = fields;
    unsigned.push(minimal(network.evm_id().ok_or(Error::Verification)? as u128));
    unsigned.push(vec![]);
    unsigned.push(vec![]);
    let hash = Keccak256::digest(encode(&unsigned));
    let mut sig = [0u8; 64];
    for (index, offset) in [(7, 0), (8, 32)] {
        let scalar = rlp.at(index).map_err(invalid)?.data().map_err(invalid)?;
        if scalar.is_empty() || scalar.len() > 32 || scalar[0] == 0 {
            return Err(Error::Verification);
        }
        sig[offset + 32 - scalar.len()..offset + 32].copy_from_slice(scalar);
    }
    secp_verify(hash.into(), &sig, &tx.public_key)?;
    // Recovery parity is also authenticated; a valid r/s with a flipped v is not accepted.
    use bitcoin::secp256k1::{
        ecdsa::{RecoverableSignature, RecoveryId},
        Message, Secp256k1,
    };
    let signature = RecoverableSignature::from_compact(
        &sig,
        RecoveryId::from_i32((v - base) as i32).map_err(invalid)?,
    )
    .map_err(invalid)?;
    let recovered = Secp256k1::verification_only()
        .recover_ecdsa(&Message::from_digest(hash.into()), &signature)
        .map_err(invalid)?;
    if hex::encode(recovered.serialize()) != tx.public_key {
        return Err(Error::Verification);
    }
    // Require canonical RLP, including the outer length and integer encodings.
    let mut canonical = unsigned_fields(t, tx)?;
    canonical.push(minimal(v as u128));
    canonical.push(
        rlp.at(7)
            .map_err(invalid)?
            .data()
            .map_err(invalid)?
            .to_vec(),
    );
    canonical.push(
        rlp.at(8)
            .map_err(invalid)?
            .data()
            .map_err(invalid)?
            .to_vec(),
    );
    if encode(&canonical) != raw {
        return Err(Error::Verification);
    }
    Ok(format!("0x{}", hex::encode(Keccak256::digest(raw))))
}
