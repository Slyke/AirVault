//! Restrictive Bitcoin adapter. Wallet Core signs; rust-bitcoin/libsecp256k1 verifies.
use crate::{
    model::{Account, Network},
    protocol::{self, Payload, Request, Response},
    Error, Result,
};
use ::bitcoin::{
    absolute, consensus,
    hashes::Hash,
    psbt::Psbt,
    secp256k1::{ecdsa::Signature, Message, Secp256k1},
    sighash::SighashCache,
    transaction, Address, Amount, EcdsaSighashType, OutPoint, ScriptBuf, Sequence, Transaction,
    TxIn, TxOut, Witness,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::HashSet, str::FromStr};
use tw_coin_entry::{
    coin_context::CoinContext, coin_entry::CoinEntry, derivation::DerivationWithPath,
};
use tw_keypair::tw::PublicKeyType;
use uuid::Uuid;
use zeroize::Zeroizing;

const MAX_MONEY: u64 = 21_000_000 * 100_000_000;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputView {
    pub outpoint: String,
    pub address: String,
    pub satoshis: u64,
    pub account_ids: Vec<Uuid>,
    pub sequence: u32,
    pub sighash: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputView {
    pub address: String,
    pub satoshis: u64,
    pub known_account: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multisig: Option<Vec<crate::multisig_psbt::Status>>,
    pub request_id: Uuid,
    pub digest: String,
    pub network: Network,
    pub inputs: Vec<InputView>,
    pub outputs: Vec<OutputView>,
    pub fee_sats: u64,
    pub fee_rate: f64,
    pub total_input_sats: u64,
    pub lock_time: u32,
    pub version: i32,
    pub external_state: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<crate::chains::transactions::Details>,
}
fn checked_sum(mut values: impl Iterator<Item = u64>) -> Result<u64> {
    values.try_fold(0u64, |sum, value| {
        sum.checked_add(value)
            .filter(|v| *v <= MAX_MONEY)
            .ok_or(Error::Invalid("Bitcoin amount overflow"))
    })
}
pub fn parse(request: &Request) -> Result<Psbt> {
    if !request.network.is_bitcoin() || request.kind != "bitcoin-psbt" {
        return Err(Error::Invalid("Bitcoin request network"));
    }
    let psbt = Psbt::deserialize(&request.validate()?)
        .map_err(|_| Error::Invalid("PSBT serialization"))?;
    if psbt.version != 0
        || psbt.inputs.len() != psbt.unsigned_tx.input.len()
        || psbt.outputs.len() != psbt.unsigned_tx.output.len()
        || psbt.inputs.is_empty()
        || psbt.inputs.len() > 100
        || psbt.outputs.is_empty()
        || psbt.outputs.len() > 100
    {
        return Err(Error::Unsupported("PSBT version or input/output count"));
    }
    if ![1, 2].contains(&psbt.unsigned_tx.version.0) {
        return Err(Error::Unsupported("transaction version"));
    }
    Ok(psbt)
}
pub fn inspect(request: &Request, accounts: &[Account]) -> Result<Inspection> {
    let psbt = parse(request)?;
    let mut inputs = vec![];
    let mut statuses = vec![];
    let multisig = psbt.inputs.iter().any(|i| i.witness_script.is_some());
    let mut seen = HashSet::new();
    for (index, (txin, input)) in psbt.unsigned_tx.input.iter().zip(&psbt.inputs).enumerate() {
        if !seen.insert(txin.previous_output) {
            return Err(Error::Invalid("duplicate input"));
        }
        if !txin.script_sig.is_empty()
            || !txin.witness.is_empty()
            || input.final_script_sig.is_some()
            || input.final_script_witness.is_some()
            || (!multisig && !input.partial_sigs.is_empty())
        {
            return Err(Error::Unsupported("already signed PSBT input"));
        }
        let prev = input.non_witness_utxo.as_ref().ok_or(Error::Invalid(
            "each input requires the full previous transaction to verify its value",
        ))?;
        if prev.compute_txid() != txin.previous_output.txid {
            return Err(Error::Invalid("previous transaction hash mismatch"));
        }
        let output = prev
            .output
            .get(txin.previous_output.vout as usize)
            .ok_or(Error::Invalid("previous output index"))?;
        if let Some(witness) = &input.witness_utxo {
            if witness != output {
                return Err(Error::Invalid("conflicting UTXO data"));
            }
        }
        if multisig {
            let policy = crate::multisig_psbt::policy(&psbt, index, request.network, accounts)?;
            statuses.push(crate::multisig_psbt::validate_signatures(
                &psbt,
                index,
                policy,
                request.network,
            )?);
        } else if !output.script_pubkey.is_p2wpkh()
            || input.redeem_script.is_some()
            || input.witness_script.is_some()
            || input.tap_internal_key.is_some()
        {
            return Err(Error::Unsupported(
                "only native SegWit P2WPKH inputs are currently enabled",
            ));
        }
        if input
            .sighash_type
            .is_some_and(|s| s.to_u32() != EcdsaSighashType::All as u32)
        {
            return Err(Error::Unsupported("only SIGHASH_ALL is permitted"));
        }
        let address = Address::from_script(&output.script_pubkey, request.network.native())
            .map_err(|_| Error::Invalid("input script"))?
            .to_string();
        let account_ids = accounts
            .iter()
            .filter(|a| a.network == request.network && a.address == address)
            .map(|a| a.id)
            .collect();
        inputs.push(InputView {
            outpoint: txin.previous_output.to_string(),
            address,
            satoshis: output.value.to_sat(),
            account_ids,
            sequence: txin.sequence.0,
            sighash: "ALL".into(),
        });
    }
    let mut outputs = vec![];
    for output in &psbt.unsigned_tx.output {
        let address = Address::from_script(&output.script_pubkey, request.network.native())
            .map_err(|_| Error::Unsupported("non-address outputs are disabled"))?
            .to_string();
        if output.value.to_sat() == 0 {
            return Err(Error::Invalid("zero-value output"));
        }
        outputs.push(OutputView {
            known_account: accounts
                .iter()
                .any(|a| a.network == request.network && a.address == address),
            address,
            satoshis: output.value.to_sat(),
        });
    }
    let total_input_sats = checked_sum(inputs.iter().map(|i| i.satoshis))?;
    let total_output_sats = checked_sum(outputs.iter().map(|o| o.satoshis))?;
    let fee_sats = total_input_sats
        .checked_sub(total_output_sats)
        .ok_or(Error::Invalid("outputs exceed inputs"))?;
    let witness_bytes: usize = if multisig {
        statuses
            .iter()
            .map(|s| {
                3 + 74 * s.required as usize + 34 * s.total + 3 + if s.total > 7 { 2 } else { 0 }
            })
            .sum()
    } else {
        109 * inputs.len()
    };
    let estimated_vbytes = psbt.unsigned_tx.vsize() + (witness_bytes + 2).div_ceil(4);
    let fee_rate = fee_sats as f64 / estimated_vbytes as f64;
    if fee_sats > 1_000_000 || fee_rate > 500.0 {
        return Err(Error::Invalid(
            "fee exceeds the limit (1,000,000 sat or 500 sat/vB)",
        ));
    }
    Ok(Inspection {
        multisig: if multisig { Some(statuses) } else { None },
        request_id: request.request_id,
        digest: request.binding()?,
        network: request.network,
        inputs,
        outputs,
        fee_sats,
        fee_rate,
        total_input_sats,
        lock_time: psbt.unsigned_tx.lock_time.to_consensus_u32(),
        version: psbt.unsigned_tx.version.0,
        external_state: true,
        chain: None,
    })
}
struct BitcoinContext;
impl CoinContext for BitcoinContext {
    fn public_key_type(&self) -> PublicKeyType {
        PublicKeyType::Secp256k1
    }
    fn address_hasher(&self) -> Option<tw_hash::hasher::Hasher> {
        None
    }
    fn hrp(&self) -> Option<String> {
        Some("bc".into())
    }
    fn p2pkh_prefix(&self) -> Option<u8> {
        Some(0)
    }
    fn p2sh_prefix(&self) -> Option<u8> {
        Some(5)
    }
    fn ss58_prefix(&self) -> Option<u16> {
        None
    }
    fn derivations(&self) -> &[DerivationWithPath] {
        &[]
    }
}
pub fn sign(
    request: &Request,
    accounts: &[Account],
    keys: &[Zeroizing<[u8; 32]>],
) -> Result<Response> {
    inspect(request, accounts)?;
    let bytes = request.validate()?;
    // Borrow secrets; never put an owned private-key copy in a protobuf buffer.
    let input = tw_proto::BitcoinV2::Proto::SigningInput {
        private_keys: keys.iter().map(|k| Cow::Borrowed(k.as_slice())).collect(),
        transaction: tw_proto::BitcoinV2::Proto::mod_SigningInput::OneOftransaction::psbt(
            tw_proto::BitcoinV2::Proto::Psbt {
                psbt: Cow::Borrowed(&bytes),
            },
        ),
        ..Default::default()
    };
    let output = tw_bitcoin::entry::BitcoinEntry.sign(&BitcoinContext, input);
    if output.encoded.is_empty() {
        return Err(Error::Verification);
    }
    let tx: Transaction =
        consensus::deserialize(&output.encoded).map_err(|_| Error::Verification)?;
    let response = Response {
        format: "airvault-signature".into(),
        version: 1,
        request_id: request.request_id,
        request_digest: request.binding()?,
        network: request.network,
        payload: Payload::new(&output.encoded),
        payload_sha256: protocol::digest(&output.encoded),
        txid: tx.compute_txid().to_string(),
    };
    verify(request, &response, accounts)?;
    Ok(response)
}
pub fn verify(request: &Request, response: &Response, accounts: &[Account]) -> Result<Inspection> {
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
    if protocol::digest(&raw) != response.payload_sha256 {
        return Err(Error::Verification);
    }
    let signed: Transaction = consensus::deserialize(&raw).map_err(|_| Error::Verification)?;
    let psbt = parse(request)?;
    let unsigned = &psbt.unsigned_tx;
    if signed.version != unsigned.version
        || signed.lock_time != unsigned.lock_time
        || signed.output != unsigned.output
        || signed.input.len() != unsigned.input.len()
        || signed.compute_txid().to_string() != response.txid
    {
        return Err(Error::Verification);
    }
    for (actual, expected) in signed.input.iter().zip(&unsigned.input) {
        if actual.previous_output != expected.previous_output
            || actual.sequence != expected.sequence
            || !actual.script_sig.is_empty()
        {
            return Err(Error::Verification);
        }
    }
    if inspection.multisig.is_some() {
        crate::multisig_psbt::verify_witnesses(request, &signed, accounts)?;
        return Ok(inspection);
    }
    let secp = Secp256k1::verification_only();
    let mut cache = SighashCache::new(&signed);
    for (index, (actual, expected)) in signed.input.iter().zip(&unsigned.input).enumerate() {
        if actual.previous_output != expected.previous_output
            || actual.sequence != expected.sequence
            || !actual.script_sig.is_empty()
            || actual.witness.len() != 2
        {
            return Err(Error::Verification);
        }
        let witness: Vec<_> = actual.witness.iter().collect();
        let signature = witness[0];
        if signature.last() != Some(&(EcdsaSighashType::All as u8)) {
            return Err(Error::Verification);
        }
        let public = ::bitcoin::CompressedPublicKey::from_slice(witness[1])
            .map_err(|_| Error::Verification)?;
        let prev = psbt.inputs[index]
            .non_witness_utxo
            .as_ref()
            .ok_or(Error::Verification)?;
        let output = &prev.output[actual.previous_output.vout as usize];
        if Address::p2wpkh(&public, request.network.native()).script_pubkey()
            != output.script_pubkey
        {
            return Err(Error::Verification);
        }
        let hash = cache
            .p2wpkh_signature_hash(
                index,
                &output.script_pubkey,
                output.value,
                EcdsaSighashType::All,
            )
            .map_err(|_| Error::Verification)?;
        let sig = Signature::from_der(&signature[..signature.len() - 1])
            .map_err(|_| Error::Verification)?;
        let mut low = sig;
        low.normalize_s();
        if sig != low {
            return Err(Error::Verification);
        }
        secp.verify_ecdsa(&Message::from_digest(hash.to_byte_array()), &sig, &public.0)
            .map_err(|_| Error::Verification)?;
    }
    Ok(inspection)
}
/// Reconstruct an interoperable signed PSBT only from an independently verified response.
/// Single-key signatures remain in standard partial_sigs so other wallets may finalize them.
pub fn signed_psbt(
    request: &Request,
    response: &Response,
    accounts: &[Account],
) -> Result<Vec<u8>> {
    verify(request, response, accounts)?;
    let mut psbt = parse(request)?;
    let tx: Transaction =
        consensus::deserialize(&response.payload.decode()?).map_err(|_| Error::Verification)?;
    let multisig = psbt.inputs.iter().any(|i| i.witness_script.is_some());
    for (input, signed) in psbt.inputs.iter_mut().zip(tx.input) {
        if multisig {
            // A received .avsig may have no retained signing-round PSBT. Export a valid finalized PSBT.
            input.final_script_witness = Some(signed.witness);
            input.partial_sigs.clear();
            input.sighash_type = None;
            input.redeem_script = None;
            input.witness_script = None;
            input.bip32_derivation.clear();
        } else {
            let stack: Vec<_> = signed.witness.iter().collect();
            let public =
                ::bitcoin::PublicKey::from_slice(stack[1]).map_err(|_| Error::Verification)?;
            let signature = ::bitcoin::ecdsa::Signature::from_slice(stack[0])
                .map_err(|_| Error::Verification)?;
            input.partial_sigs.insert(public, signature);
        }
    }
    Ok(psbt.serialize())
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Utxo {
    pub transaction_hex: String,
    pub vout: u32,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Recipient {
    pub address: String,
    pub satoshis: u64,
}
pub fn construct(
    network: Network,
    utxos: Vec<Utxo>,
    recipients: Vec<Recipient>,
    change: &str,
    fee_sats: u64,
    accounts: &[Account],
) -> Result<Request> {
    if !network.is_bitcoin() {
        return Err(Error::Invalid("Bitcoin network"));
    }
    if utxos.is_empty() || utxos.len() > 100 || recipients.is_empty() || recipients.len() > 99 {
        return Err(Error::Invalid("input/output count"));
    }
    let mut previous = vec![];
    let mut inputs = vec![];
    let mut total = 0u64;
    for utxo in utxos {
        if utxo.transaction_hex.len() > protocol::MAX_PAYLOAD_BYTES * 2 {
            return Err(Error::Invalid("previous transaction size"));
        }
        let bytes =
            hex::decode(utxo.transaction_hex).map_err(|_| Error::Invalid("transaction hex"))?;
        let tx: Transaction =
            consensus::deserialize(&bytes).map_err(|_| Error::Invalid("previous transaction"))?;
        let output = tx
            .output
            .get(utxo.vout as usize)
            .ok_or(Error::Invalid("UTXO output index"))?;
        let address = Address::from_script(&output.script_pubkey, network.native())
            .map_err(|_| Error::Invalid("UTXO address"))?
            .to_string();
        if !accounts
            .iter()
            .any(|a| a.network == network && a.address == address)
        {
            return Err(Error::Invalid("UTXO does not belong to a known account"));
        }
        total = checked_sum([total, output.value.to_sat()].into_iter())?;
        inputs.push(TxIn {
            previous_output: OutPoint {
                txid: tx.compute_txid(),
                vout: utxo.vout,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::new(),
        });
        previous.push((tx.clone(), output.clone()));
    }
    let mut outputs = vec![];
    for recipient in recipients {
        let address = Address::from_str(&recipient.address)
            .map_err(|_| Error::Invalid("recipient address"))?
            .require_network(network.native())
            .map_err(|_| Error::Invalid("recipient network"))?;
        if recipient.satoshis < 546 {
            return Err(Error::Invalid("recipient amount below initial dust policy"));
        }
        outputs.push(TxOut {
            value: Amount::from_sat(recipient.satoshis),
            script_pubkey: address.script_pubkey(),
        });
    }
    let outgoing = checked_sum(outputs.iter().map(|o| o.value.to_sat()).chain([fee_sats]))?;
    let change_value = total
        .checked_sub(outgoing)
        .ok_or(Error::Invalid("insufficient input value"))?;
    if change_value > 0 {
        if change_value < 546 {
            return Err(Error::Invalid(
                "change below dust policy; adjust amount or fee",
            ));
        }
        if !accounts
            .iter()
            .any(|a| a.network == network && a.address == change)
        {
            return Err(Error::Invalid("change must be a locally known address"));
        }
        let address = Address::from_str(change)
            .map_err(|_| Error::Invalid("change address"))?
            .require_network(network.native())
            .map_err(|_| Error::Invalid("change network"))?;
        outputs.push(TxOut {
            value: Amount::from_sat(change_value),
            script_pubkey: address.script_pubkey(),
        });
    }
    let tx = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: inputs,
        output: outputs,
    };
    let mut psbt =
        Psbt::from_unsigned_tx(tx).map_err(|_| Error::Invalid("unsigned transaction"))?;
    for (input, (prev, output)) in psbt.inputs.iter_mut().zip(previous) {
        input.non_witness_utxo = Some(prev);
        if output.script_pubkey.is_p2wsh() {
            let account = accounts
                .iter()
                .find(|a| {
                    a.network == network
                        && a.multisig.as_ref().is_some_and(|p| {
                            p.script(network)
                                .is_ok_and(|s| s.to_p2wsh() == output.script_pubkey)
                        })
                })
                .ok_or(Error::Invalid("unknown multisig input"))?;
            let policy = account.multisig.as_ref().ok_or(Error::Verification)?;
            input.witness_script = Some(policy.script(network)?);
            for (key, origin) in policy.derived(network)? {
                input.bip32_derivation.insert(key.inner, origin);
            }
        }
        if output.script_pubkey.is_p2wpkh() {
            if let Some(account) = accounts.iter().find(|a| {
                a.network == network
                    && !a.path.is_empty()
                    && Address::from_script(&output.script_pubkey, network.native())
                        .is_ok_and(|address| address.to_string() == a.address)
            }) {
                input.bip32_derivation.insert(
                    ::bitcoin::PublicKey::from_str(&account.public_key)
                        .map_err(|_| Error::Invalid("account public key"))?
                        .inner,
                    (
                        ::bitcoin::bip32::Fingerprint::from_str(&account.fingerprint)
                            .map_err(|_| Error::Invalid("account fingerprint"))?,
                        ::bitcoin::bip32::DerivationPath::from_str(&account.path)
                            .map_err(|_| Error::Invalid("account derivation path"))?,
                    ),
                );
            }
        }
        input.witness_utxo = Some(output);
        input.sighash_type = Some(EcdsaSighashType::All.into());
    }
    // Include public change origins so other PSBT wallets can identify registered change.
    for (metadata, output) in psbt.outputs.iter_mut().zip(&psbt.unsigned_tx.output) {
        let address = Address::from_script(&output.script_pubkey, network.native())
            .map_err(|_| Error::Invalid("output address"))?
            .to_string();
        if let Some(account) = accounts
            .iter()
            .find(|a| a.network == network && a.address == address && a.multisig.is_some())
        {
            let policy = account.multisig.as_ref().ok_or(Error::Verification)?;
            policy.validate_account(account)?;
            metadata.witness_script = Some(policy.script(network)?);
            for (key, origin) in policy.derived(network)? {
                metadata.bip32_derivation.insert(key.inner, origin);
            }
        }
    }
    for (metadata, output) in psbt.outputs.iter_mut().zip(&psbt.unsigned_tx.output) {
        if !output.script_pubkey.is_p2wpkh() {
            continue;
        }
        if let Some(account) = accounts.iter().find(|a| {
            a.network == network
                && !a.path.is_empty()
                && Address::from_script(&output.script_pubkey, network.native())
                    .is_ok_and(|address| address.to_string() == a.address)
        }) {
            metadata.bip32_derivation.insert(
                ::bitcoin::PublicKey::from_str(&account.public_key)
                    .map_err(|_| Error::Invalid("account public key"))?
                    .inner,
                (
                    ::bitcoin::bip32::Fingerprint::from_str(&account.fingerprint)
                        .map_err(|_| Error::Invalid("account fingerprint"))?,
                    ::bitcoin::bip32::DerivationPath::from_str(&account.path)
                        .map_err(|_| Error::Invalid("account derivation path"))?,
                ),
            );
        }
    }
    let request = Request::bitcoin(network, &psbt.serialize(), "Bitcoin transfer".into());
    inspect(&request, accounts)?;
    Ok(request)
}
