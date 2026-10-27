//! PSBT signatures are independently verified before merge, storage, finalization or broadcast.
use crate::{
    bitcoin as adapter,
    model::{Account, Network},
    multisig::Policy,
    protocol::{self, Payload, Request, Response},
    Error, Result,
};
use bitcoin::{
    consensus, ecdsa,
    hashes::Hash,
    psbt::Psbt,
    secp256k1::{Message, Secp256k1, SecretKey},
    sighash::SighashCache,
    EcdsaSighashType, PublicKey, Transaction, Witness,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Progress {
    pub request_id: Uuid,
    pub payload: Payload,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub required: u8,
    pub total: usize,
    pub signed: usize,
    pub fingerprints: Vec<String>,
}

pub fn is_multisig(request: &Request) -> Result<bool> {
    if !request.network.is_bitcoin() {
        return Ok(false);
    }
    Ok(adapter::parse(request)?
        .inputs
        .iter()
        .any(|i| i.witness_script.is_some()))
}
pub fn policy<'a>(
    psbt: &Psbt,
    index: usize,
    network: Network,
    accounts: &'a [Account],
) -> Result<&'a Policy> {
    let input = &psbt.inputs[index];
    let previous = input
        .non_witness_utxo
        .as_ref()
        .ok_or(Error::Invalid("PSBT needs full previous transactions"))?;
    let txin = &psbt.unsigned_tx.input[index];
    if previous.compute_txid() != txin.previous_output.txid {
        return Err(Error::Verification);
    }
    let output = previous
        .output
        .get(txin.previous_output.vout as usize)
        .ok_or(Error::Verification)?;
    if input.witness_utxo.as_ref().is_some_and(|o| o != output)
        || input.redeem_script.is_some()
        || input.tap_internal_key.is_some()
    {
        return Err(Error::Verification);
    }
    let script = input.witness_script.as_ref().ok_or(Error::Invalid(
        "all inputs must use the registered native SegWit multisig policy",
    ))?;
    if script.to_p2wsh() != output.script_pubkey {
        return Err(Error::Invalid(
            "witness script does not match the spent output",
        ));
    }
    let address = bitcoin::Address::from_script(&output.script_pubkey, network.native())
        .map_err(|_| Error::Verification)?
        .to_string();
    for account in accounts
        .iter()
        .filter(|a| a.network == network && a.address == address)
    {
        if let Some(policy) = &account.multisig {
            policy.validate_account(account)?;
            if policy.script(network)? == *script {
                return Ok(policy);
            }
        }
    }
    Err(Error::Invalid(
        "register this multisig wallet and address index before importing its transaction",
    ))
}
fn message(psbt: &Psbt, index: usize) -> Result<Message> {
    let input = &psbt.inputs[index];
    let previous = input.non_witness_utxo.as_ref().ok_or(Error::Verification)?;
    let output = previous
        .output
        .get(psbt.unsigned_tx.input[index].previous_output.vout as usize)
        .ok_or(Error::Verification)?;
    let hash = SighashCache::new(&psbt.unsigned_tx)
        .p2wsh_signature_hash(
            index,
            input.witness_script.as_ref().ok_or(Error::Verification)?,
            output.value,
            EcdsaSighashType::All,
        )
        .map_err(|_| Error::Verification)?;
    Ok(Message::from_digest(hash.to_byte_array()))
}
pub fn validate_signatures(
    psbt: &Psbt,
    index: usize,
    policy: &Policy,
    network: Network,
) -> Result<Status> {
    let input = &psbt.inputs[index];
    if input.sighash_type.is_some_and(|s| s.to_u32() != 1)
        || input.final_script_sig.is_some()
        || input.final_script_witness.is_some()
    {
        return Err(Error::Invalid("use an unfinalized PSBT with SIGHASH_ALL"));
    }
    let keys = policy.derived(network)?;
    let message = message(psbt, index)?;
    let secp = Secp256k1::verification_only();
    let mut fingerprints = vec![];
    for (key, signature) in &input.partial_sigs {
        let origin = keys
            .iter()
            .find(|k| k.0 == *key)
            .ok_or(Error::Invalid("signature from an unknown cosigner"))?;
        let mut low = signature.signature;
        low.normalize_s();
        if !key.compressed
            || signature.sighash_type != EcdsaSighashType::All
            || low != signature.signature
        {
            return Err(Error::Verification);
        }
        secp.verify_ecdsa(&message, &signature.signature, &key.inner)
            .map_err(|_| Error::Verification)?;
        fingerprints.push(origin.1 .0.to_string());
    }
    Ok(Status {
        required: policy.threshold,
        total: keys.len(),
        signed: input.partial_sigs.len(),
        fingerprints,
    })
}
pub fn decode(bytes: &[u8]) -> Result<Psbt> {
    if bytes.len() > protocol::MAX_EXCHANGE_BYTES {
        return Err(Error::Invalid("PSBT size"));
    }
    let raw = if bytes.starts_with(b"psbt\xff") {
        bytes.to_vec()
    } else {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(
                bytes
                    .iter()
                    .copied()
                    .filter(|b| !b.is_ascii_whitespace())
                    .collect::<Vec<_>>(),
            )
            .map_err(|_| Error::Invalid("PSBT must be binary or base64"))?
    };
    if raw.len() > protocol::MAX_PAYLOAD_BYTES {
        return Err(Error::Invalid("PSBT payload size"));
    }
    Psbt::deserialize(&raw).map_err(|_| Error::Invalid("PSBT serialization"))
}
pub fn request_with(request: &Request, psbt: &Psbt) -> Request {
    let mut result = request.clone();
    let bytes = psbt.serialize();
    result.payload = Payload::new(&bytes);
    result.payload_sha256 = protocol::digest(&bytes);
    result
}
pub fn merge(
    request: &Request,
    current: &Psbt,
    incoming: &Psbt,
    accounts: &[Account],
) -> Result<Psbt> {
    adapter::inspect(&request_with(request, incoming), accounts)?;
    if current.unsigned_tx != incoming.unsigned_tx || current.inputs.len() != incoming.inputs.len()
    {
        return Err(Error::Invalid("PSBT belongs to a different transaction"));
    }
    let mut merged = current.clone();
    for (target, source) in merged.inputs.iter_mut().zip(&incoming.inputs) {
        if target.non_witness_utxo != source.non_witness_utxo
            || target.witness_script != source.witness_script
        {
            return Err(Error::Invalid("PSBT input metadata changed"));
        }
        for (key, sig) in &source.partial_sigs {
            if target.partial_sigs.get(key).is_some_and(|old| old != sig) {
                return Err(Error::Invalid("conflicting cosigner signature"));
            }
            target.partial_sigs.insert(*key, *sig);
        }
    }
    adapter::inspect(&request_with(request, &merged), accounts)?;
    Ok(merged)
}
pub fn sign(
    request: &Request,
    current: &Psbt,
    accounts: &[Account],
    keys: &[(usize, Zeroizing<[u8; 32]>)],
) -> Result<Psbt> {
    adapter::inspect(&request_with(request, current), accounts)?;
    let mut result = current.clone();
    let secp = Secp256k1::new();
    let mut added = 0;
    for (index, key) in keys {
        let policy = policy(&result, *index, request.network, accounts)?;
        let mut secret = SecretKey::from_slice(&**key).map_err(|_| Error::Verification)?;
        let public = PublicKey::new(bitcoin::secp256k1::PublicKey::from_secret_key(
            &secp, &secret,
        ));
        let signed = if policy
            .derived(request.network)?
            .iter()
            .any(|k| k.0 == public)
        {
            Some(ecdsa::Signature::sighash_all(
                secp.sign_ecdsa(&message(&result, *index)?, &secret),
            ))
        } else {
            None
        };
        secret.non_secure_erase();
        let signature = signed.ok_or(Error::Verification)?;
        if result.inputs[*index]
            .partial_sigs
            .insert(public, signature)
            .is_none()
        {
            added += 1;
        }
    }
    if added == 0 {
        return Err(Error::Invalid("this signer has no new signatures to add"));
    }
    adapter::inspect(&request_with(request, &result), accounts)?;
    Ok(result)
}
pub fn finalize(request: &Request, psbt: &Psbt, accounts: &[Account]) -> Result<Option<Response>> {
    adapter::inspect(&request_with(request, psbt), accounts)?;
    let mut signed = psbt.unsigned_tx.clone();
    for index in 0..psbt.inputs.len() {
        let policy = policy(psbt, index, request.network, accounts)?;
        let status = validate_signatures(psbt, index, policy, request.network)?;
        if status.signed < status.required as usize {
            return Ok(None);
        }
        let mut witness = Witness::new();
        witness.push([]);
        let mut count = 0;
        for (key, _) in policy.derived(request.network)? {
            if let Some(sig) = psbt.inputs[index].partial_sigs.get(&key) {
                witness.push(sig.to_vec());
                count += 1;
                if count == policy.threshold {
                    break;
                }
            }
        }
        witness.push(policy.script(request.network)?.as_bytes());
        signed.input[index].witness = witness;
    }
    let raw = consensus::serialize(&signed);
    let response = Response {
        format: "airvault-signature".into(),
        version: 1,
        request_id: request.request_id,
        request_digest: request.binding()?,
        network: request.network,
        payload: Payload::new(&raw),
        payload_sha256: protocol::digest(&raw),
        txid: signed.compute_txid().to_string(),
    };
    adapter::verify(request, &response, accounts)?;
    Ok(Some(response))
}
pub fn verify_witnesses(
    request: &Request,
    signed: &Transaction,
    accounts: &[Account],
) -> Result<()> {
    let mut psbt = adapter::parse(request)?;
    for (index, txin) in signed.input.iter().enumerate() {
        let policy = policy(&psbt, index, request.network, accounts)?.clone();
        let witness: Vec<_> = txin.witness.iter().collect();
        let script = policy.script(request.network)?;
        if witness.len() != policy.threshold as usize + 2
            || !witness[0].is_empty()
            || witness.last() != Some(&script.as_bytes())
        {
            return Err(Error::Verification);
        }
        let message = message(&psbt, index)?;
        let keys = policy.derived(request.network)?;
        let secp = Secp256k1::verification_only();
        let mut next = 0;
        psbt.inputs[index].partial_sigs.clear();
        for raw in &witness[1..witness.len() - 1] {
            let sig = ecdsa::Signature::from_slice(raw).map_err(|_| Error::Verification)?;
            let mut found = None;
            while next < keys.len() {
                let key = keys[next].0;
                next += 1;
                if secp
                    .verify_ecdsa(&message, &sig.signature, &key.inner)
                    .is_ok()
                {
                    found = Some(key);
                    break;
                }
            }
            psbt.inputs[index]
                .partial_sigs
                .insert(found.ok_or(Error::Verification)?, sig);
        }
        validate_signatures(&psbt, index, &policy, request.network)?;
    }
    Ok(())
}
