use airvault_core::{
    bitcoin as adapter,
    model::{Mode, Network},
    multisig::{Backup, Policy},
    multisig_psbt,
    session::Session,
};
use bitcoin::{
    absolute, consensus, transaction, Address, Amount, OutPoint, ScriptBuf, Sequence, Transaction,
    TxIn, TxOut, Witness,
};
use std::{collections::HashMap, str::FromStr};
use uuid::Uuid;
const PASSWORD: &str = "public-multisig-test";
struct Setup {
    _dir: tempfile::TempDir,
    signers: Vec<Session>,
    coordinator: Session,
    policy: Policy,
    identities: Vec<Uuid>,
}
impl Setup {
    fn new(threshold: u8) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut signers = vec![];
        let mut identities = vec![];
        let mut cosigners = vec![];
        for n in 0..3 {
            let mut signer = Session::new(dir.path().join(format!("signer-{n}.avvault")));
            signer.create(PASSWORD, Mode::Offline).unwrap();
            let phrase = bip39::Mnemonic::from_entropy(&[n as u8; 16])
                .unwrap()
                .to_string();
            let id = signer
                .mutate(|v| v.import_phrase("Public disposable fixture", &phrase, None))
                .unwrap();
            cosigners.push(
                signer
                    .vault()
                    .unwrap()
                    .multisig_cosigner(id, Network::Regtest, 0, None)
                    .unwrap(),
            );
            identities.push(id);
            signers.push(signer);
        }
        let policy = Policy {
            threshold,
            cosigners,
            branch: 0,
            index: 0,
        };
        for (signer, id) in signers.iter_mut().zip(&identities) {
            signer
                .mutate(|v| {
                    v.register_multisig("Shared", Network::Regtest, policy.clone(), Some(*id), None)
                })
                .unwrap();
        }
        let mut coordinator = Session::new(dir.path().join("coordinator.avvault"));
        coordinator.create(PASSWORD, Mode::Proxy).unwrap();
        coordinator
            .import_public(&signers[0].public_bytes().unwrap())
            .unwrap();
        Self {
            _dir: dir,
            signers,
            coordinator,
            policy,
            identities,
        }
    }
    fn request(&mut self) -> Uuid {
        let accounts = self.coordinator.snapshot().accounts;
        let script = self.policy.script(Network::Regtest).unwrap().to_p2wsh();
        let funding = Transaction {
            version: transaction::Version::TWO,
            lock_time: absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![
                TxOut {
                    value: Amount::from_sat(100_000),
                    script_pubkey: script.clone(),
                },
                TxOut {
                    value: Amount::from_sat(90_000),
                    script_pubkey: script,
                },
            ],
        };
        let request = adapter::construct(
            Network::Regtest,
            vec![0, 1]
                .into_iter()
                .map(|vout| adapter::Utxo {
                    transaction_hex: hex::encode(consensus::serialize(&funding)),
                    vout,
                })
                .collect(),
            vec![adapter::Recipient {
                address: accounts[0].address.clone(),
                satoshis: 50_000,
            }],
            &accounts[0].address,
            1000,
            &accounts,
        )
        .unwrap();
        self.coordinator
            .import_request(&serde_json::to_vec(&request).unwrap())
            .unwrap()
    }
    fn cosign(&mut self, signer: usize, bytes: &[u8]) -> Vec<u8> {
        let session = &mut self.signers[signer];
        let id = session.import_psbt(bytes, Network::Regtest).unwrap();
        let review = session.review(id).unwrap();
        session
            .sign_multisig(review.token, true, PASSWORD, &HashMap::new(), &[])
            .unwrap();
        session.psbt_bytes(id).unwrap()
    }
}
#[test]
fn separate_signers_merge_finalize_restore_and_reject_tampering() {
    let mut setup = Setup::new(2);
    let request_id = setup.request();
    let unsigned = setup.coordinator.psbt_bytes(request_id).unwrap();
    let first = setup.cosign(0, &unsigned);
    setup
        .coordinator
        .import_psbt(&first, Network::Regtest)
        .unwrap();
    assert!(setup.coordinator.snapshot().responses.is_empty());
    assert!(setup.coordinator.transaction_bytes(request_id).is_err());
    let review = setup.coordinator.review(request_id).unwrap();
    assert!(review
        .inspection
        .multisig
        .unwrap()
        .iter()
        .all(|s| s.signed == 1 && s.required == 2));
    // Parallel signing: both devices started with the unsigned transaction.
    let second = setup.cosign(1, &unsigned);
    setup
        .coordinator
        .import_psbt(&second, Network::Regtest)
        .unwrap();
    let snapshot = setup.coordinator.snapshot();
    assert_eq!(snapshot.responses.len(), 1);
    assert_eq!(
        setup.coordinator.transaction_bytes(request_id).unwrap(),
        snapshot.responses[0].payload.decode().unwrap()
    );
    let external_dir = tempfile::tempdir().unwrap();
    let mut external = Session::new(external_dir.path().join("imported.avvault"));
    external.create(PASSWORD, Mode::Proxy).unwrap();
    external
        .import_public(&setup.coordinator.public_bytes().unwrap())
        .unwrap();
    external
        .import_request(&serde_json::to_vec(&snapshot.pending[0]).unwrap())
        .unwrap();
    external
        .import_response(&serde_json::to_vec(&snapshot.responses[0]).unwrap())
        .unwrap();
    let exported =
        bitcoin::psbt::Psbt::deserialize(&external.psbt_bytes(request_id).unwrap()).unwrap();
    assert!(exported
        .inputs
        .iter()
        .all(|i| i.final_script_witness.is_some() && i.partial_sigs.is_empty()));
    assert_eq!(
        bitcoin::consensus::serialize(&exported.extract_tx().unwrap()),
        external.transaction_bytes(request_id).unwrap()
    );
    let response = snapshot.responses[0].clone();
    adapter::verify(&snapshot.pending[0], &response, &snapshot.accounts).unwrap();
    let mut tx: Transaction = consensus::deserialize(&response.payload.decode().unwrap()).unwrap();
    assert!(tx.input.iter().all(|i| i.witness.len() == 4));
    let mut bad = response.clone();
    tx.input[1].witness = Witness::new();
    let raw = consensus::serialize(&tx);
    bad.payload = airvault_core::protocol::Payload::new(&raw);
    bad.payload_sha256 = airvault_core::protocol::digest(&raw);
    assert!(adapter::verify(&snapshot.pending[0], &bad, &snapshot.accounts).is_err());
    // Invalid partials and another network are rejected without changing stored progress.
    let mut invalid = multisig_psbt::decode(&first).unwrap();
    let key = *invalid.inputs[0].partial_sigs.keys().next().unwrap();
    let signature = invalid.inputs[0].partial_sigs[&key];
    invalid.inputs[1].partial_sigs.insert(key, signature); // wrong input sighash
    let before = setup.coordinator.psbt_bytes(request_id).unwrap();
    assert!(setup
        .coordinator
        .import_psbt(&invalid.serialize(), Network::Regtest)
        .is_err());
    assert!(setup
        .coordinator
        .import_psbt(&first, Network::Testnet)
        .is_err());
    assert_eq!(setup.coordinator.psbt_bytes(request_id).unwrap(), before);
    setup.coordinator.lock();
    setup.coordinator.unlock(PASSWORD).unwrap();
    assert_eq!(setup.coordinator.psbt_bytes(request_id).unwrap(), before);
    let backup = Backup::new("Recovery", Network::Regtest, setup.policy.clone()).unwrap();
    let parsed = Backup::parse(&serde_json::to_vec(&backup).unwrap()).unwrap();
    assert_eq!(
        parsed.address,
        setup.signers[0].snapshot().accounts[0].address
    );
    let mut tampered = parsed.clone();
    tampered.policy.threshold = 1;
    assert!(Backup::parse(&serde_json::to_vec(&tampered).unwrap()).is_err());
}
#[test]
fn signatures_are_counted_per_input_and_approvals_are_invalidated() {
    let mut setup = Setup::new(3);
    let id = setup.request();
    let unsigned = setup.coordinator.psbt_bytes(id).unwrap();
    let first = setup.cosign(0, &unsigned);
    let second = setup.cosign(1, &unsigned);
    let mut third = multisig_psbt::decode(&setup.cosign(2, &unsigned)).unwrap();
    third.inputs[1].partial_sigs.clear();
    for bytes in [&first, &second, &third.serialize()] {
        setup
            .coordinator
            .import_psbt(bytes, Network::Regtest)
            .unwrap();
    }
    assert!(setup.coordinator.snapshot().responses.is_empty());
    let progress = setup
        .coordinator
        .review(id)
        .unwrap()
        .inspection
        .multisig
        .unwrap();
    assert_eq!((progress[0].signed, progress[1].signed), (3, 2));
    let signer_id = setup.signers[0].snapshot().pending[0].request_id;
    let review = setup.signers[0].review(signer_id).unwrap();
    setup.signers[0]
        .import_psbt(&second, Network::Regtest)
        .unwrap();
    assert!(setup.signers[0]
        .sign_multisig(review.token, true, PASSWORD, &HashMap::new(), &[])
        .is_err());
    let complete = setup.signers[2]
        .psbt_bytes(setup.signers[2].snapshot().pending[0].request_id)
        .unwrap();
    setup
        .coordinator
        .import_psbt(&complete, Network::Regtest)
        .unwrap();
    assert_eq!(setup.coordinator.snapshot().responses.len(), 1);
}
#[test]
fn policy_ownership_duplicate_keys_and_descriptor_recovery() {
    let mut setup = Setup::new(2);
    let original = setup.policy.address(Network::Regtest).unwrap();
    let mut reversed = setup.policy.clone();
    reversed.cosigners.reverse();
    assert_eq!(original, reversed.address(Network::Regtest).unwrap());
    let mut duplicate = setup.policy.clone();
    duplicate.cosigners[1] = duplicate.cosigners[0].clone();
    assert!(duplicate.validate(Network::Regtest).is_err());
    assert!(setup.policy.validate(Network::Bitcoin).is_err());
    let mut wrong = setup.policy.clone();
    wrong.cosigners.remove(0);
    let identity = setup.identities[0];
    assert!(setup.signers[0]
        .mutate(|v| v.register_multisig(
            "Wrong signer",
            Network::Regtest,
            wrong,
            Some(identity),
            None
        ))
        .is_err());
    let mut next = setup.policy.clone();
    next.index = 1;
    next.branch = 1;
    assert_ne!(original, next.address(Network::Regtest).unwrap());
    let phrase = bip39::Mnemonic::from_entropy(&[0u8; 16])
        .unwrap()
        .to_string();
    let mut restored = Session::new(setup._dir.path().join("restored.avvault"));
    restored.create(PASSWORD, Mode::Offline).unwrap();
    restored
        .mutate(|v| {
            let id = v.import_phrase("Restored", &phrase, None)?;
            v.register_multisig(
                "Recovered",
                Network::Regtest,
                setup.policy.clone(),
                Some(id),
                None,
            )
        })
        .unwrap();
    assert_eq!(original, restored.snapshot().accounts[0].address);
    assert_eq!(
        airvault_core::multisig::descriptor_checksum("raw(deadbeef)").unwrap(),
        "raw(deadbeef)#89f8spxm"
    );
    Address::from_str(&original)
        .unwrap()
        .require_network(bitcoin::Network::Regtest)
        .unwrap();
}
