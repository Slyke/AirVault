use super::{fields, Details, Transaction};
use crate::{
    chains::{self, bytes, invalid, keys, Context},
    model::Network,
    Error, Result,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use tw_coin_entry::coin_entry::CoinEntry;
use tw_proto::Solana::Proto as sol;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Transfer {
    pub to: String,
    pub amount: String,
    pub recent_blockhash: String,
    #[serde(default)]
    pub asset: Option<Token>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Token {
    pub mint: String,
    pub symbol: String,
    pub decimals: u8,
}
const SYSTEM: &str = "11111111111111111111111111111111";
const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const ASSOCIATED: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
pub fn associated(owner: &str, mint: &str, network: Network) -> Result<String> {
    let owner = keys::address_bytes(owner, network)?;
    let mint = keys::address_bytes(mint, network)?;
    let token = keys::address_bytes(TOKEN, network)?;
    let program = keys::address_bytes(ASSOCIATED, network)?;
    for bump in (0..=255u8).rev() {
        let mut hash = Sha256::new();
        hash.update(&owner);
        hash.update(&token);
        hash.update(&mint);
        hash.update([bump]);
        hash.update(&program);
        hash.update(b"ProgramDerivedAddress");
        let digest: [u8; 32] = hash.finalize().into();
        if curve25519_dalek::edwards::CompressedEdwardsY(digest)
            .decompress()
            .is_none()
        {
            return Ok(bs58::encode(digest).into_string());
        }
    }
    Err(Error::Invalid("associated token account"))
}
fn message(
    t: &Transfer,
    tx: &Transaction,
    network: Network,
) -> Result<sol::mod_RawMessage::MessageLegacy<'static>> {
    let amount: u64 = chains::units(&t.amount)?.try_into().map_err(invalid)?;
    if amount == 0 || t.to == tx.sender {
        return Err(Error::Invalid("recipient or amount"));
    }
    keys::address_bytes(&t.to, network)?;
    keys::address_bytes(&t.recent_blockhash, network)?;
    let (accounts, data, indices, program, readonly) = if let Some(token) = &t.asset {
        if token.decimals > 18
            || token.symbol.is_empty()
            || token.symbol.len() > 16
            || !token.symbol.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(Error::Invalid("token metadata"));
        }
        let source = associated(&tx.sender, &token.mint, network)?;
        let target = associated(&t.to, &token.mint, network)?;
        let mut data = vec![12];
        data.extend_from_slice(&amount.to_le_bytes());
        data.push(token.decimals);
        (
            vec![
                tx.sender.clone(),
                source,
                target,
                token.mint.clone(),
                TOKEN.into(),
            ],
            data,
            vec![1, 3, 2, 0],
            4,
            2,
        )
    } else {
        let mut data = 2u32.to_le_bytes().to_vec();
        data.extend_from_slice(&amount.to_le_bytes());
        (
            vec![tx.sender.clone(), t.to.clone(), SYSTEM.into()],
            data,
            vec![0, 1],
            2,
            1,
        )
    };
    let unique: std::collections::HashSet<_> = accounts.iter().collect();
    if unique.len() != accounts.len() {
        return Err(Error::Invalid("overlapping Solana accounts"));
    }
    Ok(sol::mod_RawMessage::MessageLegacy {
        header: Some(sol::mod_RawMessage::MessageHeader {
            num_required_signatures: 1,
            num_readonly_signed_accounts: 0,
            num_readonly_unsigned_accounts: readonly,
        }),
        account_keys: accounts.into_iter().map(Cow::Owned).collect(),
        recent_blockhash: Cow::Owned(t.recent_blockhash.clone()),
        instructions: vec![sol::mod_RawMessage::Instruction {
            program_id: program,
            accounts: indices,
            program_data: Cow::Owned(data),
        }],
    })
}
fn short(value: usize, out: &mut Vec<u8>) {
    let mut n = value;
    loop {
        let byte = (n & 127) as u8;
        n >>= 7;
        out.push(if n == 0 { byte } else { byte | 128 });
        if n == 0 {
            break;
        }
    }
}
fn wire(message: &sol::mod_RawMessage::MessageLegacy, network: Network) -> Result<Vec<u8>> {
    let header = message
        .header
        .as_ref()
        .ok_or(Error::Invalid("message header"))?;
    let mut out = vec![
        header.num_required_signatures as u8,
        header.num_readonly_signed_accounts as u8,
        header.num_readonly_unsigned_accounts as u8,
    ];
    short(message.account_keys.len(), &mut out);
    for account in &message.account_keys {
        out.extend_from_slice(&keys::address_bytes(account, network)?);
    }
    out.extend_from_slice(&keys::address_bytes(&message.recent_blockhash, network)?);
    short(message.instructions.len(), &mut out);
    for instruction in &message.instructions {
        out.push(instruction.program_id as u8);
        short(instruction.accounts.len(), &mut out);
        out.extend(instruction.accounts.iter().map(|v| *v as u8));
        short(instruction.program_data.len(), &mut out);
        out.extend_from_slice(&instruction.program_data);
    }
    Ok(out)
}
pub fn inspect(t: &Transfer, tx: &Transaction, network: Network) -> Result<Details> {
    message(t, tx, network)?;
    let (symbol, decimals) = t
        .asset
        .as_ref()
        .map(|a| (a.symbol.as_str(), a.decimals))
        .unwrap_or(("SOL", 9));
    let mut fields = fields(t)?;
    if let Some(asset) = &t.asset {
        fields.push((
            "Source token account".into(),
            associated(&tx.sender, &asset.mint, network)?,
        ));
        fields.push((
            "Destination token account".into(),
            associated(&t.to, &asset.mint, network)?,
        ));
    }
    Ok(Details {
        title: "Solana transfer".into(),
        amount: format!(
            "{} {symbol}",
            chains::display_units(chains::units(&t.amount)?, decimals)
        ),
        fee: "Network base fee · 1 signature · no priority fee".into(),
        fields,
    })
}
pub fn sign(t: &Transfer, tx: &Transaction, network: Network, key: &[u8]) -> Result<Vec<u8>> {
    let raw_message = sol::RawMessage {
        message: sol::mod_RawMessage::OneOfmessage::legacy(message(t, tx, network)?),
        ..Default::default()
    };
    let input = sol::SigningInput {
        private_key: Cow::Borrowed(key),
        raw_message: Some(raw_message),
        tx_encoding: sol::Encoding::Base64,
        ..Default::default()
    };
    let result = tw_solana::entry::SolanaEntry.sign(&Context(network), input);
    if result.error != tw_proto::Common::Proto::SigningError::OK {
        return Err(Error::Invalid("Solana signing failed"));
    }
    STANDARD.decode(result.encoded.as_ref()).map_err(invalid)
}
pub fn verify(t: &Transfer, tx: &Transaction, network: Network, raw: &[u8]) -> Result<String> {
    let expected = wire(&message(t, tx, network)?, network)?;
    if raw.len() != 65 + expected.len() || raw[0] != 1 || raw[65..] != expected {
        return Err(Error::Verification);
    }
    let public =
        ed25519_dalek::VerifyingKey::from_bytes(&bytes(&chains::hex_bytes(&tx.public_key)?)?)
            .map_err(invalid)?;
    let signature = ed25519_dalek::Signature::from_slice(&raw[1..65]).map_err(invalid)?;
    public
        .verify_strict(&raw[65..], &signature)
        .map_err(|_| Error::Verification)?;
    Ok(bs58::encode(&raw[1..65]).into_string())
}
