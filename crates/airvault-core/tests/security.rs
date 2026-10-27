use airvault_core::{
    bitcoin as adapter,
    model::{Mode, Network, VaultData},
    protocol::{self, Payload, PublicBundle, Request},
    session::Session,
    storage::{self, OpenVault},
    Error,
};
use bitcoin::{
    absolute, consensus, transaction, Address, Amount, OutPoint, ScriptBuf, Sequence, Transaction,
    TxIn, TxOut, Witness,
};
use std::{collections::HashMap, str::FromStr};
use uuid::Uuid;
const PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const PASSWORD: &str = "public-test-password";
const CUSTOM: &str = "public-custom-password";
fn vault() -> OpenVault {
    OpenVault::create(PASSWORD, VaultData::default()).unwrap()
}
fn funded_request(accounts: &[airvault_core::model::Account]) -> Request {
    let account = &accounts[0];
    let address = Address::from_str(&account.address)
        .unwrap()
        .require_network(account.network.native())
        .unwrap();
    let previous = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::null(),
            script_sig: ScriptBuf::from_bytes(vec![1, 1]),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: address.script_pubkey(),
        }],
    };
    adapter::construct(
        account.network,
        vec![adapter::Utxo {
            transaction_hex: hex::encode(consensus::serialize(&previous)),
            vout: 0,
        }],
        vec![adapter::Recipient {
            address: accounts[1].address.clone(),
            satoshis: 40_000,
        }],
        &account.address,
        1000,
        accounts,
    )
    .unwrap()
}
fn signer(dir: &std::path::Path) -> Session {
    let mut s = Session::new(dir.join("vault.avvault"));
    s.create(PASSWORD, Mode::Offline).unwrap();
    s.mutate(|v| {
        let id = v.import_phrase("Public test vector", PHRASE, None)?;
        v.derive_account(id, "Input", Network::Regtest, "m/84'/1'/0'/0/0", None)?;
        v.derive_account(id, "Recipient", Network::Regtest, "m/84'/1'/0'/0/1", None)?;
        Ok(())
    })
    .unwrap();
    s
}
#[test]
fn encrypted_storage_password_rotation_custom_isolation_and_backup_integrity() {
    let mut v = vault();
    let default = v.import_phrase("Default root", PHRASE, None).unwrap();
    let custom = v
        .import_phrase("Protected root", PHRASE, Some(CUSTOM))
        .unwrap();
    let custom_root = v
        .data
        .identities
        .iter()
        .find(|i| i.id == custom)
        .unwrap()
        .root_id;
    assert!(v.decrypt_record(custom_root, None).is_err());
    assert!(v.decrypt_record(custom_root, Some(PASSWORD)).is_err());
    assert_eq!(
        &*v.decrypt_record(custom_root, Some(CUSTOM)).unwrap(),
        PHRASE.as_bytes()
    );
    let bytes = v.encrypted_bytes().unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains(PHRASE));
    assert!(!text.contains("Protected root"));
    assert!(OpenVault::unlock(&bytes, "wrong-password").is_err());
    let mut restored = OpenVault::unlock(&bytes, PASSWORD).unwrap();
    assert_eq!(restored.data.identities.len(), 2);
    let record_before = serde_json::to_vec(&restored.data.records).unwrap();
    restored
        .change_password(PASSWORD, "replacement-password")
        .unwrap();
    assert_eq!(
        record_before,
        serde_json::to_vec(&restored.data.records).unwrap()
    );
    let rotated = restored.encrypted_bytes().unwrap();
    assert!(OpenVault::unlock(&rotated, PASSWORD).is_err());
    assert!(OpenVault::unlock(&rotated, "replacement-password").is_ok());
    let mut modified: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    modified["metadata"]["ciphertext"][0] =
        serde_json::json!((modified["metadata"]["ciphertext"][0].as_u64().unwrap() + 1) % 256);
    assert!(OpenVault::unlock(&serde_json::to_vec(&modified).unwrap(), PASSWORD).is_err());
    let mut bad_kdf: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    bad_kdf["master"]["memoryKib"] = serde_json::json!(u32::MAX);
    assert!(OpenVault::unlock(&serde_json::to_vec(&bad_kdf).unwrap(), PASSWORD).is_err());
    assert_ne!(default, custom);
}
#[test]
fn official_bip84_bip86_vectors_and_unicode_passphrases() {
    let mut v = vault();
    let id = v.import_phrase("Vector", PHRASE, None).unwrap();
    let vectors = [
        (
            "m/84'/0'/0'/0/0",
            "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu",
        ),
        (
            "m/86'/0'/0'/0/0",
            "bc1p5cyxnuxmeuwuvkwfem96lqzszd02n6xdcjrs20cac6yqjjwudpxqkedrcr",
        ),
        ("m/44'/0'/0'/0/0", "1LqBGSKuX5yYUonjxT5qGfpUsXKYYWeabA"),
        ("m/49'/0'/0'/0/0", "37VucYSaXLCAsxYyAPfbSi9eh4iEcbShgf"),
    ];
    for (path, expected) in vectors {
        let account = v
            .derive_account(id, "Vector", Network::Bitcoin, path, None)
            .unwrap();
        assert_eq!(
            v.data
                .accounts
                .iter()
                .find(|a| a.id == account)
                .unwrap()
                .address,
            expected
        );
    }
    let root = v.data.roots[0].id;
    let composed = v.add_identity(root, "Composed", "é", None).unwrap();
    let decomposed = v
        .add_identity(root, "Decomposed", "e\u{301}", None)
        .unwrap();
    v.derive_account(composed, "A", Network::Testnet, "m/84'/1'/0'/0/0", None)
        .unwrap();
    v.derive_account(decomposed, "B", Network::Testnet, "m/84'/1'/0'/0/0", None)
        .unwrap();
    assert_eq!(v.data.accounts[4].address, v.data.accounts[5].address);
    assert!(v
        .derive_account(id, "Wrong coin", Network::Testnet, "m/84'/0'/0'/0/0", None)
        .is_err());
    assert!(v.verify_recovery(root, PHRASE, None).unwrap());
}
#[test]
fn file_round_trip_signs_with_wallet_core_and_independently_verifies() {
    let signer_dir = tempfile::tempdir().unwrap();
    let proxy_dir = tempfile::tempdir().unwrap();
    let mut signer = signer(signer_dir.path());
    let request = funded_request(&signer.snapshot().accounts);
    let request_bytes = serde_json::to_vec(&request).unwrap();
    let inspection = adapter::inspect(&request, &signer.snapshot().accounts).unwrap();
    assert_eq!(inspection.fee_sats, 1000);
    assert_eq!(inspection.outputs[1].satoshis, 59_000);
    signer.import_request(&request_bytes).unwrap();
    assert!(matches!(
        signer.import_request(&request_bytes),
        Err(Error::Duplicate)
    ));
    let review = signer.review(request.request_id).unwrap();
    let response = signer
        .sign(review.token, true, PASSWORD, &HashMap::new(), &[])
        .unwrap();
    assert_eq!(
        adapter::verify(&request, &response, &signer.snapshot().accounts)
            .unwrap()
            .fee_sats,
        1000
    );
    assert!(signer
        .sign(review.token, true, PASSWORD, &HashMap::new(), &[])
        .is_err());
    #[cfg(feature = "network")]
    {
        let mut proxy = Session::new(proxy_dir.path().join("proxy.avvault"));
        proxy.create(PASSWORD, Mode::Proxy).unwrap();
        proxy
            .import_public(&signer.public_bytes().unwrap())
            .unwrap();
        proxy.import_request(&request_bytes).unwrap();
        proxy
            .import_response(&serde_json::to_vec(&response).unwrap())
            .unwrap();
        assert!(proxy.vault().unwrap().data.records.is_empty());
        assert_eq!(proxy.snapshot().history[0].action, "verified");
        let review = proxy.review(request.request_id).unwrap();
        assert!(matches!(
            proxy.sign(review.token, true, PASSWORD, &HashMap::new(), &[]),
            Err(Error::Mode)
        ));
    }
    let _ = proxy_dir;
    let mut tampered = response.clone();
    let mut tx: Transaction = consensus::deserialize(&tampered.payload.decode().unwrap()).unwrap();
    tx.output[0].value = Amount::from_sat(40_001);
    let raw = consensus::serialize(&tx);
    tampered.payload = Payload::new(&raw);
    tampered.payload_sha256 = protocol::digest(&raw);
    tampered.txid = tx.compute_txid().to_string();
    assert!(adapter::verify(&request, &tampered, &signer.snapshot().accounts).is_err());
    let mut wrong_network = response.clone();
    wrong_network.network = Network::Bitcoin;
    assert!(adapter::verify(&request, &wrong_network, &signer.snapshot().accounts).is_err());
}
#[test]
fn malformed_psbt_unsafe_sighash_and_fee_substitution_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let s = signer(dir.path());
    let accounts = s.snapshot().accounts;
    let request = funded_request(&accounts);
    let psbt = adapter::parse(&request).unwrap();
    let mut permissive = psbt.clone();
    permissive.inputs[0].sighash_type =
        Some(bitcoin::EcdsaSighashType::NonePlusAnyoneCanPay.into());
    assert!(adapter::inspect(
        &Request::bitcoin(Network::Regtest, &permissive.serialize(), String::new()),
        &accounts
    )
    .is_err());
    let mut previous = psbt.clone();
    previous.inputs[0].non_witness_utxo.as_mut().unwrap().output[0].value =
        Amount::from_sat(1_000_000);
    assert!(adapter::inspect(
        &Request::bitcoin(Network::Regtest, &previous.serialize(), String::new()),
        &accounts
    )
    .is_err());
    let mut missing = psbt.clone();
    missing.inputs[0].non_witness_utxo = None;
    assert!(adapter::inspect(
        &Request::bitcoin(Network::Regtest, &missing.serialize(), String::new()),
        &accounts
    )
    .is_err());
    let mut conflicting = psbt.clone();
    conflicting.inputs[0].witness_utxo.as_mut().unwrap().value = Amount::from_sat(100_001);
    assert!(adapter::inspect(
        &Request::bitcoin(Network::Regtest, &conflicting.serialize(), String::new()),
        &accounts
    )
    .is_err());
    let mut duplicate = psbt.clone();
    duplicate
        .unsigned_tx
        .input
        .push(duplicate.unsigned_tx.input[0].clone());
    duplicate.inputs.push(duplicate.inputs[0].clone());
    assert!(adapter::inspect(
        &Request::bitcoin(Network::Regtest, &duplicate.serialize(), String::new()),
        &accounts
    )
    .is_err());
    let mut metadata = request.clone();
    metadata.description = "Send zero BTC to a trusted address".into();
    assert_eq!(
        adapter::inspect(&metadata, &accounts).unwrap().outputs[0].satoshis,
        40_000
    );
    let mut unknown: serde_json::Value =
        serde_json::from_slice(&serde_json::to_vec(&request).unwrap()).unwrap();
    unknown["execute"] = serde_json::json!("sign_without_review");
    assert!(Request::parse(&serde_json::to_vec(&unknown).unwrap()).is_err());
    assert!(Request::parse(&vec![b'x'; protocol::MAX_EXCHANGE_BYTES + 1]).is_err());
}
#[test]
fn startup_lock_one_time_phrase_and_confirmation_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.avvault");
    let mut session = Session::new(path.clone());
    session.create(PASSWORD, Mode::Offline).unwrap();
    let ceremony = session.generate(24, &"abandon ".repeat(11), None).unwrap();
    assert!(ceremony.phrase.starts_with(&"abandon ".repeat(11)));
    session
        .confirm_phrase(ceremony.token, &ceremony.phrase, "Generated", None)
        .unwrap();
    assert!(session
        .confirm_phrase(ceremony.token, &ceremony.phrase, "Replay", None)
        .is_err());
    assert!(!serde_json::to_string(&session.snapshot())
        .unwrap()
        .contains(&ceremony.phrase));
    session.lock();
    assert!(session.snapshot().roots.is_empty());
    assert!(matches!(session.generate(24, "", None), Err(Error::Locked)));
    let mut restarted = Session::new(path);
    assert!(!restarted.snapshot().unlocked);
    restarted.unlock(PASSWORD).unwrap();
    assert_eq!(restarted.snapshot().roots.len(), 1);
}
#[test]
fn approval_is_single_use_and_cannot_be_forged_or_bypassed() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = signer(dir.path());
    let request = funded_request(&s.snapshot().accounts);
    s.import_request(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    assert!(s
        .sign(Uuid::new_v4(), true, PASSWORD, &HashMap::new(), &[])
        .is_err());
    let review = s.review(request.request_id).unwrap();
    assert!(s
        .sign(review.token, false, PASSWORD, &HashMap::new(), &[])
        .is_err());
    assert!(s
        .sign(review.token, true, PASSWORD, &HashMap::new(), &[])
        .is_err());
    let review = s.review(request.request_id).unwrap();
    assert!(s
        .sign(review.token, true, "wrong-password", &HashMap::new(), &[])
        .is_err());
    assert!(s.snapshot().responses.is_empty());
    assert!(s.snapshot().history.is_empty());
}
#[test]
fn backups_are_portable_and_require_explicit_authorization() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = signer(dir.path());
    let backup = dir.path().join("copy.avvault");
    assert!(s.backup(&backup, PASSWORD, false).is_err());
    assert!(s.backup(&backup, "wrong-password", true).is_err());
    s.backup(&backup, PASSWORD, true).unwrap();
    let other = tempfile::tempdir().unwrap();
    let mut restore = Session::new(other.path().join("restored.avvault"));
    restore.restore(&backup, PASSWORD, true).unwrap();
    assert!(!restore.snapshot().initialized);
    restore.restore(&backup, PASSWORD, false).unwrap();
    assert!(!restore.snapshot().unlocked);
    restore.unlock(PASSWORD).unwrap();
    assert_eq!(
        restore.snapshot().accounts[0].address,
        s.snapshot().accounts[0].address
    );
    assert!(restore.restore(&backup, PASSWORD, false).is_err());
    let public = s.public_bytes().unwrap();
    PublicBundle::parse(&public).unwrap();
    assert!(!String::from_utf8_lossy(&public).contains(PHRASE));
    assert!(storage::read_bounded(&backup, 1).is_err());
}

#[test]
fn public_bundle_rejects_recomputed_checksum_with_mismatched_public_key() {
    let dir = tempfile::tempdir().unwrap();
    let s = signer(dir.path());
    let mut accounts = s.vault().unwrap().public_accounts();
    accounts[0].public_key = accounts[1].public_key.clone();
    let forged = PublicBundle::new(accounts).unwrap();
    assert!(PublicBundle::parse(&serde_json::to_vec(&forged).unwrap()).is_err());
}
