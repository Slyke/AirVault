use super::{fields, secp_verify, Details, Transaction};
use crate::{
    chains::{self, invalid, keys, Context},
    model::Network,
    Error, Result,
};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use tw_coin_entry::coin_entry::CoinEntry;
use tw_proto::Cosmos::Proto as cosmos;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Transfer {
    pub to: String,
    pub amount: String,
    pub account_number: u64,
    pub sequence: u64,
    pub gas_limit: u64,
    pub fee: String,
    #[serde(default)]
    pub memo: String,
    #[serde(default)]
    pub action: Action,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    #[default]
    Send,
    Delegate,
    Undelegate,
    ClaimRewards,
}
#[derive(Clone, PartialEq, Message)]
struct Any {
    #[prost(string, tag = "1")]
    url: String,
    #[prost(bytes = "vec", tag = "2")]
    value: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct Coin {
    #[prost(string, tag = "1")]
    denom: String,
    #[prost(string, tag = "2")]
    amount: String,
}
#[derive(Clone, PartialEq, Message)]
struct Send {
    #[prost(string, tag = "1")]
    from: String,
    #[prost(string, tag = "2")]
    to: String,
    #[prost(message, repeated, tag = "3")]
    coins: Vec<Coin>,
}
#[derive(Clone, PartialEq, Message)]
struct Stake {
    #[prost(string, tag = "1")]
    from: String,
    #[prost(string, tag = "2")]
    to: String,
    #[prost(message, optional, tag = "3")]
    coin: Option<Coin>,
}
#[derive(Clone, PartialEq, Message)]
struct Body {
    #[prost(message, repeated, tag = "1")]
    messages: Vec<Any>,
    #[prost(string, tag = "2")]
    memo: String,
}
#[derive(Clone, PartialEq, Message)]
struct Key {
    #[prost(bytes = "vec", tag = "1")]
    key: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct Single {
    #[prost(uint32, tag = "1")]
    mode: u32,
}
#[derive(Clone, PartialEq, Message)]
struct Mode {
    #[prost(message, optional, tag = "1")]
    single: Option<Single>,
}
#[derive(Clone, PartialEq, Message)]
struct Signer {
    #[prost(message, optional, tag = "1")]
    key: Option<Any>,
    #[prost(message, optional, tag = "2")]
    mode: Option<Mode>,
    #[prost(uint64, tag = "3")]
    sequence: u64,
}
#[derive(Clone, PartialEq, Message)]
struct Fee {
    #[prost(message, repeated, tag = "1")]
    amount: Vec<Coin>,
    #[prost(uint64, tag = "2")]
    gas: u64,
}
#[derive(Clone, PartialEq, Message)]
struct Auth {
    #[prost(message, repeated, tag = "1")]
    signers: Vec<Signer>,
    #[prost(message, optional, tag = "2")]
    fee: Option<Fee>,
}
#[derive(Clone, PartialEq, Message)]
struct Doc {
    #[prost(bytes = "vec", tag = "1")]
    body: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    auth: Vec<u8>,
    #[prost(string, tag = "3")]
    chain: String,
    #[prost(uint64, tag = "4")]
    number: u64,
}
#[derive(Clone, PartialEq, Message)]
struct Raw {
    #[prost(bytes = "vec", tag = "1")]
    body: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    auth: Vec<u8>,
    #[prost(bytes = "vec", repeated, tag = "3")]
    signatures: Vec<Vec<u8>>,
}
fn coin(amount: &str) -> Coin {
    Coin {
        denom: "uatom".into(),
        amount: amount.into(),
    }
}
fn parts(t: &Transfer, tx: &Transaction) -> Result<(Vec<u8>, Vec<u8>)> {
    let message = match t.action {
        Action::Send => Any {
            url: "/cosmos.bank.v1beta1.MsgSend".into(),
            value: Send {
                from: tx.sender.clone(),
                to: t.to.clone(),
                coins: vec![coin(&t.amount)],
            }
            .encode_to_vec(),
        },
        _ => Any {
            url: match t.action {
                Action::Delegate => "/cosmos.staking.v1beta1.MsgDelegate",
                Action::Undelegate => "/cosmos.staking.v1beta1.MsgUndelegate",
                _ => "/cosmos.distribution.v1beta1.MsgWithdrawDelegatorReward",
            }
            .into(),
            value: Stake {
                from: tx.sender.clone(),
                to: t.to.clone(),
                coin: if matches!(t.action, Action::ClaimRewards) {
                    None
                } else {
                    Some(coin(&t.amount))
                },
            }
            .encode_to_vec(),
        },
    };
    let body = Body {
        messages: vec![message],
        memo: t.memo.clone(),
    }
    .encode_to_vec();
    let auth = Auth {
        signers: vec![Signer {
            key: Some(Any {
                url: "/cosmos.crypto.secp256k1.PubKey".into(),
                value: Key {
                    key: chains::hex_bytes(&tx.public_key)?,
                }
                .encode_to_vec(),
            }),
            mode: Some(Mode {
                single: Some(Single { mode: 1 }),
            }),
            sequence: t.sequence,
        }],
        fee: Some(Fee {
            amount: vec![coin(&t.fee)],
            gas: t.gas_limit,
        }),
    }
    .encode_to_vec();
    Ok((body, auth))
}
pub fn inspect(t: &Transfer, _tx: &Transaction, network: Network) -> Result<Details> {
    if matches!(t.action, Action::Send) {
        keys::address_bytes(&t.to, network)?;
    } else {
        let (hrp, data) = bech32::decode(&t.to).map_err(invalid)?;
        if hrp.as_str() != "cosmosvaloper"
            || data.len() != 20
            || bech32::encode::<bech32::Bech32>(hrp, &data).map_err(invalid)? != t.to
        {
            return Err(Error::Invalid("Cosmos validator address"));
        }
    }
    if (!matches!(t.action, Action::ClaimRewards) && chains::units(&t.amount)? == 0)
        || t.gas_limit == 0
        || t.gas_limit > 10_000_000
        || t.memo.len() > 256
        || t.memo.chars().any(char::is_control)
    {
        return Err(Error::Invalid("Cosmos amount, gas or memo"));
    }
    let fee = chains::units(&t.fee)?;
    let mut fields = fields(t)?;
    fields.push(("Chain ID".into(), "cosmoshub-4".into()));
    Ok(Details {
        title: match t.action {
            Action::Send => "Cosmos transfer",
            Action::Delegate => "Stake ATOM",
            Action::Undelegate => "Unstake ATOM",
            Action::ClaimRewards => "Claim staking rewards",
        }
        .into(),
        amount: if matches!(t.action, Action::ClaimRewards) {
            "Accrued staking rewards".into()
        } else {
            format!(
                "{} ATOM",
                chains::display_units(chains::units(&t.amount)?, 6)
            )
        },
        fee: format!("{} ATOM", chains::display_units(fee, 6)),
        fields,
    })
}
pub fn sign(t: &Transfer, tx: &Transaction, network: Network, key: &[u8]) -> Result<Vec<u8>> {
    inspect(t, tx, network)?;
    let amount = || cosmos::Amount {
        denom: Cow::Borrowed("uatom"),
        amount: Cow::Borrowed(&t.amount),
    };
    use cosmos::mod_Message::OneOfmessage_oneof as O;
    let operation = match t.action {
        Action::Send => O::send_coins_message(cosmos::mod_Message::Send {
            from_address: Cow::Borrowed(&tx.sender),
            to_address: Cow::Borrowed(&t.to),
            amounts: vec![amount()],
            ..Default::default()
        }),
        Action::Delegate => O::stake_message(cosmos::mod_Message::Delegate {
            delegator_address: Cow::Borrowed(&tx.sender),
            validator_address: Cow::Borrowed(&t.to),
            amount: Some(amount()),
            ..Default::default()
        }),
        Action::Undelegate => O::unstake_message(cosmos::mod_Message::Undelegate {
            delegator_address: Cow::Borrowed(&tx.sender),
            validator_address: Cow::Borrowed(&t.to),
            amount: Some(amount()),
            ..Default::default()
        }),
        Action::ClaimRewards => {
            O::withdraw_stake_reward_message(cosmos::mod_Message::WithdrawDelegationReward {
                delegator_address: Cow::Borrowed(&tx.sender),
                validator_address: Cow::Borrowed(&t.to),
                ..Default::default()
            })
        }
    };
    let input = cosmos::SigningInput {
        signing_mode: cosmos::SigningMode::Protobuf,
        chain_id: Cow::Borrowed("cosmoshub-4"),
        account_number: t.account_number,
        sequence: t.sequence,
        private_key: Cow::Borrowed(key),
        memo: Cow::Borrowed(&t.memo),
        fee: Some(cosmos::Fee {
            amounts: vec![cosmos::Amount {
                denom: Cow::Borrowed("uatom"),
                amount: Cow::Borrowed(&t.fee),
            }],
            gas: t.gas_limit,
        }),
        messages: vec![cosmos::Message {
            message_oneof: operation,
        }],
        mode: cosmos::BroadcastMode::SYNC,
        ..Default::default()
    };
    let output = tw_cosmos::entry::CosmosEntry.sign(&Context(network), input);
    if output.error != tw_proto::Common::Proto::SigningError::OK {
        return Err(Error::Invalid("Cosmos signing failed"));
    }
    let (body, auth) = parts(t, tx)?;
    let raw = Raw {
        body,
        auth,
        signatures: vec![output.signature.into_owned()],
    }
    .encode_to_vec();
    verify(t, tx, network, &raw)?;
    Ok(raw)
}
pub fn verify(t: &Transfer, tx: &Transaction, network: Network, raw: &[u8]) -> Result<String> {
    inspect(t, tx, network)?;
    let decoded = Raw::decode(raw).map_err(invalid)?;
    let (body, auth) = parts(t, tx)?;
    if decoded.body != body
        || decoded.auth != auth
        || decoded.signatures.len() != 1
        || decoded.encode_to_vec() != raw
    {
        return Err(Error::Verification);
    }
    let digest = Sha256::digest(
        Doc {
            body,
            auth,
            chain: "cosmoshub-4".into(),
            number: t.account_number,
        }
        .encode_to_vec(),
    );
    secp_verify(digest.into(), &decoded.signatures[0], &tx.public_key)?;
    Ok(hex::encode_upper(Sha256::digest(raw)))
}
