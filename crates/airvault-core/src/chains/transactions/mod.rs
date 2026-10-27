//! Typed native transactions. Unknown operations cannot reach a signing primitive.
pub mod cardano;
pub mod cosmos;
pub mod evm;
pub mod polkadot;
pub mod solana;
pub mod tron;
pub mod utxo;
pub mod xrp;
use super::{invalid, keys};
use crate::{
    bitcoin::{InputView, Inspection},
    model::{Account, Network},
    protocol::{self, Payload, Request, Response},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Transaction {
    pub sender: String,
    pub public_key: String,
    pub operation: Operation,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Evm(evm::Transfer),
    Solana(solana::Transfer),
    Cosmos(cosmos::Transfer),
    Xrp(xrp::Transfer),
    Tron(tron::Transfer),
    Utxo(utxo::Transfer),
    Cardano(cardano::Transfer),
    Polkadot(polkadot::Transfer),
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    pub title: String,
    pub amount: String,
    pub fee: String,
    pub fields: Vec<(String, String)>,
}
impl Transaction {
    pub fn parse(request: &Request) -> Result<Self> {
        if request.kind != "chain-transaction" || request.network.is_bitcoin() {
            return Err(Error::Invalid("transaction family"));
        }
        let tx: Self = serde_json::from_slice(&request.validate()?)?;
        if tx.public_key.len() > 130 || tx.sender.len() > 180 {
            return Err(Error::Invalid("sender metadata"));
        }
        if keys::address_from_public(&super::hex_bytes(&tx.public_key)?, request.network)?
            != tx.sender
        {
            return Err(Error::Invalid("sender public key"));
        }
        Ok(tx)
    }
    fn details(&self, network: Network) -> Result<Details> {
        match &self.operation {
            Operation::Evm(t) if network.evm_id().is_some() => evm::inspect(t, self, network),
            Operation::Solana(t) if matches!(network, Network::Solana | Network::SolanaDevnet) => {
                solana::inspect(t, self, network)
            }
            Operation::Cosmos(t) if network == Network::Cosmos => cosmos::inspect(t, self, network),
            Operation::Xrp(t) if network == Network::Xrp => xrp::inspect(t, self, network),
            Operation::Tron(t) if matches!(network, Network::Tron | Network::TronNile) => {
                tron::inspect(t, self, network)
            }
            Operation::Utxo(t) if network.is_utxo() => utxo::inspect(t, self, network),
            Operation::Cardano(t)
                if matches!(network, Network::Cardano | Network::CardanoPreprod) =>
            {
                cardano::inspect(t, self, network)
            }
            Operation::Polkadot(t) if network == Network::Polkadot => {
                polkadot::inspect(t, self, network)
            }
            _ => Err(Error::Invalid("operation does not match blockchain")),
        }
    }
}
pub fn request(network: Network, tx: Transaction) -> Result<Request> {
    tx.details(network)?;
    let raw = serde_json::to_vec(&tx)?;
    let r = Request {
        format: "airvault-request".into(),
        version: 1,
        request_id: uuid::Uuid::new_v4(),
        kind: "chain-transaction".into(),
        network,
        payload: Payload::new(&raw),
        payload_sha256: protocol::digest(&raw),
        description: "Transfer".into(),
    };
    Transaction::parse(&r)?;
    Ok(r)
}
pub fn inspect(request: &Request, accounts: &[Account]) -> Result<Inspection> {
    if request.network.is_bitcoin() {
        return crate::bitcoin::inspect(request, accounts);
    }
    let tx = Transaction::parse(request)?;
    let chain = tx.details(request.network)?;
    let token = match &tx.operation {
        Operation::Evm(evm::Transfer {
            asset:
                Some(evm::Asset::Erc20 {
                    contract,
                    symbol,
                    decimals,
                }),
            ..
        }) => Some(("erc20", contract, symbol, *decimals)),
        Operation::Solana(solana::Transfer {
            asset: Some(asset), ..
        }) => Some(("spl", &asset.mint, &asset.symbol, asset.decimals)),
        Operation::Tron(tron::Transfer {
            asset: Some(asset), ..
        }) => Some(("trc20", &asset.contract, &asset.symbol, asset.decimals)),
        _ => None,
    };
    if let Some((standard, contract, symbol, decimals)) = token {
        let found = accounts
            .iter()
            .filter(|a| {
                a.network == request.network
                    && a.address == tx.sender
                    && a.public_key == tx.public_key
            })
            .any(|a| {
                a.tokens.iter().any(|t| {
                    t.standard == standard
                        && (if request.network.evm_id().is_some() {
                            t.contract.eq_ignore_ascii_case(contract)
                        } else {
                            t.contract == *contract
                        })
                        && t.symbol == *symbol
                        && t.decimals == decimals
                })
            });
        if !found {
            return Err(Error::Invalid("Add this token to the sender account and confirm its contract and decimals before reviewing"));
        }
    }
    let ids = accounts
        .iter()
        .filter(|a| {
            a.network == request.network && a.address == tx.sender && a.public_key == tx.public_key
        })
        .map(|a| a.id)
        .collect();
    Ok(Inspection {
        multisig: None,
        request_id: request.request_id,
        digest: request.binding()?,
        network: request.network,
        inputs: vec![InputView {
            outpoint: String::new(),
            address: tx.sender,
            satoshis: 0,
            account_ids: ids,
            sequence: 0,
            sighash: "All transaction fields".into(),
        }],
        outputs: vec![],
        fee_sats: 0,
        fee_rate: 0.0,
        total_input_sats: 0,
        lock_time: 0,
        version: 1,
        external_state: true,
        chain: Some(chain),
    })
}
pub fn sign(
    request: &Request,
    accounts: &[Account],
    keys: &[Zeroizing<Vec<u8>>],
) -> Result<Response> {
    if request.network.is_bitcoin() {
        let keys: Vec<_> = keys
            .iter()
            .map(|k| super::bytes(k).map(Zeroizing::new))
            .collect::<Result<_>>()?;
        return crate::bitcoin::sign(request, accounts, &keys);
    }
    inspect(request, accounts)?;
    if keys.len() != 1 {
        return Err(Error::Invalid("one signer required"));
    }
    let tx = Transaction::parse(request)?;
    let key = &keys[0];
    let raw = match &tx.operation {
        Operation::Evm(t) => evm::sign(t, &tx, request.network, key)?,
        Operation::Solana(t) => solana::sign(t, &tx, request.network, key)?,
        Operation::Cosmos(t) => cosmos::sign(t, &tx, request.network, key)?,
        Operation::Xrp(t) => xrp::sign(t, &tx, request.network, key)?,
        Operation::Tron(t) => tron::sign(t, &tx, request.network, key)?,
        Operation::Utxo(t) => utxo::sign(t, &tx, request.network, key)?,
        Operation::Cardano(t) => cardano::sign(t, &tx, request.network, key)?,
        Operation::Polkadot(t) => polkadot::sign(t, &tx, request.network, key)?,
    };
    let txid = verify_native(&tx, request.network, &raw)?;
    let response = Response {
        format: "airvault-signature".into(),
        version: 1,
        request_id: request.request_id,
        request_digest: request.binding()?,
        network: request.network,
        payload: Payload::new(&raw),
        payload_sha256: protocol::digest(&raw),
        txid,
    };
    verify(request, &response, accounts)?;
    Ok(response)
}
fn verify_native(tx: &Transaction, network: Network, raw: &[u8]) -> Result<String> {
    match &tx.operation {
        Operation::Evm(t) => evm::verify(t, tx, network, raw),
        Operation::Solana(t) => solana::verify(t, tx, network, raw),
        Operation::Cosmos(t) => cosmos::verify(t, tx, network, raw),
        Operation::Xrp(t) => xrp::verify(t, tx, network, raw),
        Operation::Tron(t) => tron::verify(t, tx, network, raw),
        Operation::Utxo(t) => utxo::verify(t, tx, network, raw),
        Operation::Cardano(t) => cardano::verify(t, tx, network, raw),
        Operation::Polkadot(t) => polkadot::verify(t, tx, network, raw),
    }
}
pub fn verify(request: &Request, response: &Response, accounts: &[Account]) -> Result<Inspection> {
    if request.network.is_bitcoin() {
        return crate::bitcoin::verify(request, response, accounts);
    }
    let inspection = inspect(request, accounts)?;
    if response.format != "airvault-signature"
        || response.version != 1
        || response.request_id != request.request_id
        || response.request_digest != request.binding()?
        || response.network != request.network
    {
        return Err(Error::Verification);
    }
    let raw = response.payload.decode()?;
    if protocol::digest(&raw) != response.payload_sha256
        || verify_native(&Transaction::parse(request)?, request.network, &raw)? != response.txid
    {
        return Err(Error::Verification);
    }
    Ok(inspection)
}
pub fn secp_verify(digest: [u8; 32], signature: &[u8], public: &str) -> Result<()> {
    use bitcoin::secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1};
    let sig = Signature::from_compact(signature).map_err(invalid)?;
    let mut low = sig;
    low.normalize_s();
    if sig != low {
        return Err(Error::Verification);
    }
    Secp256k1::verification_only()
        .verify_ecdsa(
            &Message::from_digest(digest),
            &sig,
            &PublicKey::from_slice(&super::hex_bytes(public)?).map_err(invalid)?,
        )
        .map_err(|_| Error::Verification)
}
pub fn fields<T: Serialize>(value: &T) -> Result<Vec<(String, String)>> {
    fn flatten(prefix: String, value: serde_json::Value, fields: &mut Vec<(String, String)>) {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    flatten(
                        if prefix.is_empty() {
                            k
                        } else {
                            format!("{prefix} / {k}")
                        },
                        v,
                        fields,
                    )
                }
            }
            serde_json::Value::Array(items) => {
                for (i, v) in items.into_iter().enumerate() {
                    flatten(format!("{prefix} / {}", i + 1), v, fields)
                }
            }
            serde_json::Value::String(s) => fields.push((prefix, s)),
            other => fields.push((prefix, other.to_string())),
        }
    }
    let mut result = vec![];
    flatten(String::new(), serde_json::to_value(value)?, &mut result);
    Ok(result)
}
