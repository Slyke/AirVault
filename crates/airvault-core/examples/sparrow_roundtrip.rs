//! Manual interoperability harness. ALL seeds below are public test vectors. Never fund these wallets on mainnet.
//! Not linked into AirVault. See docs/sparrow-interoperability.md.
use airvault_core::{
    bitcoin as adapter,
    model::{Mode, Network},
    multisig::Policy,
    protocol::{self, Payload, Request, Response},
    session::Session,
};
use bitcoin::{
    absolute,
    bip32::{DerivationPath, Xpriv},
    consensus, transaction, Address, Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn,
    TxOut, Witness,
};
use serde_json::json;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
};
const PASSWORD: &str = "public-sparrow-fixture";
fn single() -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::new(dir.path().join("single.avvault"));
    s.create(PASSWORD, Mode::Offline).unwrap();
    s.mutate(|v|{let id=v.import_phrase("Public BIP39 vector","abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",None)?;
        for i in 0..2{v.derive_account(id,"Fixture",Network::Regtest,&format!("m/84'/1'/0'/0/{i}"),None)?;}Ok(())}).unwrap();
    (dir, s)
}
fn multi(local: u8) -> (tempfile::TempDir, Session, Policy) {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::new(dir.path().join("multi.avvault"));
    s.create(PASSWORD, Mode::Offline).unwrap();
    let mut keys = vec![];
    let mut local_id = None;
    for n in 0..3 {
        let phrase = bip39::Mnemonic::from_entropy(&[n; 16]).unwrap().to_string();
        let id = s
            .mutate(|v| v.import_phrase("Public multisig vector", &phrase, None))
            .unwrap();
        keys.push(
            s.vault()
                .unwrap()
                .multisig_cosigner(id, Network::Regtest, 0, None)
                .unwrap(),
        );
        if n == local {
            local_id = Some(id);
        }
    }
    let policy = Policy {
        threshold: 2,
        cosigners: keys,
        branch: 0,
        index: 0,
    };
    s.mutate(|v| {
        v.register_multisig(
            "Public 2-of-3",
            Network::Regtest,
            policy.clone(),
            local_id,
            None,
        )
    })
    .unwrap();
    (dir, s, policy)
}
fn fixture_request(s: &Session) -> Request {
    let accounts = s.snapshot().accounts;
    let address = Address::from_str(&accounts[0].address)
        .unwrap()
        .require_network(bitcoin::Network::Regtest)
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
        Network::Regtest,
        vec![adapter::Utxo {
            transaction_hex: hex::encode(consensus::serialize(&previous)),
            vout: 0,
        }],
        vec![adapter::Recipient {
            address: accounts.last().unwrap().address.clone(),
            satoshis: 40_000,
        }],
        &accounts[0].address,
        1000,
        &accounts,
    )
    .unwrap()
}
fn write_outputs(s: &Session, id: uuid::Uuid, prefix: &Path) {
    std::fs::write(prefix.with_extension("psbt"), s.psbt_bytes(id).unwrap()).unwrap();
    if let Ok(raw) = s.transaction_bytes(id) {
        std::fs::write(prefix.with_extension("txn"), hex::encode(&raw)).unwrap();
    }
}
fn sign_single(s: &mut Session, raw: &[u8]) -> uuid::Uuid {
    let id = s.import_psbt(raw, Network::Regtest).unwrap();
    let r = s.review(id).unwrap();
    s.sign(r.token, true, PASSWORD, &HashMap::new(), &[])
        .unwrap();
    id
}
fn sign_multi(s: &mut Session, raw: &[u8]) -> uuid::Uuid {
    let id = s.import_psbt(raw, Network::Regtest).unwrap();
    let r = s.review(id).unwrap();
    s.sign_multisig(r.token, true, PASSWORD, &HashMap::new(), &[])
        .unwrap();
    id
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let action = args
        .get(1)
        .expect("prepare|sign-single|sign-multi|verify-single|verify-multi");
    let dir = PathBuf::from(args.get(2).expect("fixture directory"));
    std::fs::create_dir_all(&dir).unwrap();
    match action.as_str() {
        "prepare" => {
            let (_sd, mut one) = single();
            let (_md, mut many, policy) = multi(0);
            for (name, s) in [("single", &mut one), ("multi", &mut many)] {
                let path = dir.join(format!("{name}-unsigned.psbt"));
                let request = if path.exists() {
                    Request::bitcoin(
                        Network::Regtest,
                        &std::fs::read(&path).unwrap(),
                        "Sparrow interoperability".into(),
                    )
                } else {
                    fixture_request(s)
                };
                let unsigned = request.validate().unwrap();
                std::fs::write(&path, &unsigned).unwrap();
                std::fs::write(
                    dir.join(format!("{name}-request.avreq")),
                    serde_json::to_vec_pretty(&request).unwrap(),
                )
                .unwrap();
                let id = if name == "single" {
                    sign_single(s, &unsigned)
                } else {
                    sign_multi(s, &unsigned)
                };
                write_outputs(s, id, &dir.join(format!("{name}-airvault-signed")));
            }
            let (_other, mut cosigner, _) = multi(1);
            let id = sign_multi(
                &mut cosigner,
                &std::fs::read(dir.join("multi-airvault-signed.psbt")).unwrap(),
            );
            write_outputs(&cosigner, id, &dir.join("multi-airvault-complete"));
            // Sparrow imports this watch-only descriptor; add only the public vector signer #2 for a cosigning round.
            let descriptor = airvault_core::multisig::Backup::new(
                "Public test",
                Network::Regtest,
                policy.clone(),
            )
            .unwrap()
            .receive_descriptor;
            std::fs::write(dir.join("multi-watch-descriptor.txt"), descriptor).unwrap();
            let phrase = bip39::Mnemonic::from_entropy(&[1; 16]).unwrap();
            let secp = bitcoin::secp256k1::Secp256k1::new();
            let root = Xpriv::new_master(bitcoin::Network::Regtest, &phrase.to_seed("")).unwrap();
            let origin = DerivationPath::from_str("m/48'/1'/0'/2'").unwrap();
            let account = root.derive_priv(&secp, &origin).unwrap();
            // Explicit public-vector software key for Sparrow's keystore import.
            // Descriptor import itself remains watch-only in Sparrow 2.5.5.
            std::fs::write(
                dir.join("multi-public-fixture-master.txt"),
                root.to_string(),
            )
            .unwrap();
            let metadata = json!({"singleAddress":one.snapshot().accounts[0].address,"multisigAddress":policy.address(Network::Regtest).unwrap(),"cosignerMasterFingerprint":root.fingerprint(&secp).to_string(),"publicFixtureAccountKey":bitcoin::bip32::Xpub::from_priv(&secp,&account).to_string()});
            std::fs::write(
                dir.join("fixture-info.json"),
                serde_json::to_vec_pretty(&metadata).unwrap(),
            )
            .unwrap();
            println!("Prepared public regtest fixtures.");
        }
        "sign-single" | "sign-multi" => {
            let input = dir.join(args.get(3).expect("input filename"));
            let prefix = dir.join(args.get(4).expect("output prefix"));
            let raw = std::fs::read(input).unwrap();
            if action == "sign-single" {
                let (_d, mut s) = single();
                let id = sign_single(&mut s, &raw);
                write_outputs(&s, id, &prefix);
            } else {
                let (_d, mut s, _) = multi(0);
                let id = sign_multi(&mut s, &raw);
                write_outputs(&s, id, &prefix);
            }
            println!("Signed and independently verified imported PSBT.");
        }
        "verify-single" | "verify-multi" => {
            let family = if action == "verify-single" {
                "single"
            } else {
                "multi"
            };
            let request = Request::parse(
                &std::fs::read(dir.join(format!("{family}-request.avreq"))).unwrap(),
            )
            .unwrap();
            let text =
                std::fs::read_to_string(dir.join(args.get(3).expect("Sparrow .txn filename")))
                    .unwrap();
            let raw = hex::decode(text.trim()).unwrap();
            let tx: Transaction = consensus::deserialize(&raw).unwrap();
            let response = Response {
                format: "airvault-signature".into(),
                version: 1,
                request_id: request.request_id,
                request_digest: request.binding().unwrap(),
                network: Network::Regtest,
                payload: Payload::new(&raw),
                payload_sha256: protocol::digest(&raw),
                txid: tx.compute_txid().to_string(),
            };
            let (_d, s) = if family == "single" {
                single()
            } else {
                let (d, s, _) = multi(0);
                (d, s)
            };
            adapter::verify(&request, &response, &s.snapshot().accounts).unwrap();
            println!(
                "Sparrow raw transaction independently verified: {}",
                response.txid
            );
        }
        _ => panic!("unknown action"),
    }
}
