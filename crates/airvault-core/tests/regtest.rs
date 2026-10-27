#![cfg(feature = "network")]
// Run explicitly with BITCOIND pointing to a verified local Bitcoin Core binary.
use airvault_core::{
    bitcoin as adapter,
    model::{Mode, Network},
    session::Session,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    process::{Child, Command, Stdio},
    time::Duration,
};
const PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const PASSWORD: &str = "public-regtest-password";
struct Node {
    child: Child,
    url: String,
    cookie: String,
}
impl Drop for Node {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Node {
    fn rpc(&self, method: &str, params: Value) -> Value {
        let result: Value = ureq::post(&self.url)
            .set(
                "Authorization",
                &format!("Basic {}", STANDARD.encode(&self.cookie)),
            )
            .send_json(json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .unwrap()
            .into_json()
            .unwrap();
        assert!(
            result.get("error").is_none_or(Value::is_null),
            "{method}: {:?}",
            result["error"]
        );
        result["result"].clone()
    }
}
#[test]
#[ignore = "requires a verified Bitcoin Core binary; no external peers are enabled"]
fn wallet_core_transaction_is_accepted_and_confirmed_by_bitcoin_core() {
    let (node, _node_dir) = start_node();
    let signer_dir = tempfile::tempdir().unwrap();
    let proxy_dir = tempfile::tempdir().unwrap();
    let mut signer = Session::new(signer_dir.path().join("signer.avvault"));
    signer.create(PASSWORD, Mode::Offline).unwrap();
    signer
        .mutate(|v| {
            let identity = v.import_phrase("Public regtest vector", PHRASE, None)?;
            v.derive_account(identity, "Input", Network::Regtest, "m/84'/1'/0'/0/0", None)?;
            v.derive_account(
                identity,
                "Recipient",
                Network::Regtest,
                "m/84'/1'/0'/0/1",
                None,
            )?;
            Ok(())
        })
        .unwrap();
    let accounts = signer.snapshot().accounts;
    let blocks = node.rpc("generatetoaddress", json!([101, accounts[0].address]));
    let block = node.rpc("getblock", json!([blocks[0], 2]));
    let previous = node
        .rpc(
            "getrawtransaction",
            json!([block["tx"][0]["txid"], false, blocks[0]]),
        )
        .as_str()
        .unwrap()
        .to_owned();
    let request = adapter::construct(
        Network::Regtest,
        vec![adapter::Utxo {
            transaction_hex: previous,
            vout: 0,
        }],
        vec![adapter::Recipient {
            address: accounts[1].address.clone(),
            satoshis: 100_000,
        }],
        &accounts[0].address,
        1000,
        &accounts,
    )
    .unwrap();
    save_sparrow_fixture("single-unsigned.psbt", &request.validate().unwrap());
    let mut proxy = Session::new(proxy_dir.path().join("proxy.avvault"));
    proxy.create(PASSWORD, Mode::Proxy).unwrap();
    proxy
        .import_public(&signer.public_bytes().unwrap())
        .unwrap();
    let avreq = serde_json::to_vec(&request).unwrap();
    proxy.import_request(&avreq).unwrap();
    signer.import_request(&avreq).unwrap();
    let review = signer.review(request.request_id).unwrap();
    let response = signer
        .sign(review.token, true, PASSWORD, &HashMap::new(), &[])
        .unwrap();
    proxy
        .import_response(&serde_json::to_vec(&response).unwrap())
        .unwrap();
    let signed_psbt = signer.psbt_bytes(request.request_id).unwrap();
    let finalization = node.rpc("finalizepsbt", json!([STANDARD.encode(&signed_psbt)]));
    assert_eq!(finalization["complete"], true);
    assert_eq!(
        finalization["hex"],
        hex::encode(signer.transaction_bytes(request.request_id).unwrap())
    );
    save_sparrow_fixture("single-core-signed.psbt", &signed_psbt);
    save_sparrow_fixture(
        "single-core-signed.txn",
        finalization["hex"].as_str().unwrap().as_bytes(),
    );
    let raw = hex::encode(response.payload.decode().unwrap());
    let accepted = node.rpc("testmempoolaccept", json!([[raw]]));
    assert_eq!(accepted[0]["allowed"], true, "{accepted}");
    let txid = node.rpc("sendrawtransaction", json!([raw]));
    assert_eq!(txid.as_str().unwrap(), response.txid);
    let mined = node.rpc("generatetoaddress", json!([1, accounts[0].address]));
    let confirmed = node.rpc("getrawtransaction", json!([txid, true, mined[0]]));
    assert_eq!(confirmed["confirmations"], 1);
    println!(
        "Bitcoin Core accepted and confirmed Wallet Core transaction: {}",
        response.txid
    );
}

fn start_node() -> (Node, tempfile::TempDir) {
    let binary = std::env::var("BITCOIND").expect("set BITCOIND");
    let node_dir = tempfile::tempdir().unwrap();
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    drop(socket);
    let child = Command::new(binary)
        .args([
            "-regtest",
            "-server=1",
            "-listen=0",
            "-connect=0",
            "-dnsseed=0",
            "-discover=0",
            "-rpcbind=127.0.0.1",
            "-rpcallowip=127.0.0.1",
        ])
        .arg(format!("-rpcport={port}"))
        .arg(format!("-datadir={}", node_dir.path().display()))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut node = Node {
        child,
        url: format!("http://127.0.0.1:{port}"),
        cookie: String::new(),
    };
    for _ in 0..100 {
        if let Ok(cookie) = std::fs::read_to_string(node_dir.path().join("regtest/.cookie")) {
            node.cookie = cookie;
            if ureq::post(&node.url)
                .set(
                    "Authorization",
                    &format!("Basic {}", STANDARD.encode(&node.cookie)),
                )
                .send_json(json!({"jsonrpc":"2.0","id":1,"method":"getblockchaininfo","params":[]}))
                .is_ok()
            {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!node.cookie.is_empty(), "node did not start");
    (node, node_dir)
}

#[test]
#[ignore = "requires a verified Bitcoin Core binary; no external peers are enabled"]
fn multisig_two_separate_signers_are_accepted_by_bitcoin_core() {
    use airvault_core::multisig::{Backup, Policy};
    let (node, _node_dir) = start_node();
    let directory = tempfile::tempdir().unwrap();
    let mut signers = vec![];
    let mut ids = vec![];
    let mut cosigners = vec![];
    for n in 0..3 {
        let mut signer = Session::new(directory.path().join(format!("multisig-{n}.avvault")));
        signer.create(PASSWORD, Mode::Offline).unwrap();
        let phrase = bip39::Mnemonic::from_entropy(&[n as u8; 16])
            .unwrap()
            .to_string();
        let id = signer
            .mutate(|v| v.import_phrase("Disposable regtest signer", &phrase, None))
            .unwrap();
        cosigners.push(
            signer
                .vault()
                .unwrap()
                .multisig_cosigner(id, Network::Regtest, 0, None)
                .unwrap(),
        );
        ids.push(id);
        signers.push(signer);
    }
    let policy = Policy {
        threshold: 2,
        cosigners,
        branch: 0,
        index: 0,
    };
    for (signer, id) in signers.iter_mut().zip(&ids) {
        signer
            .mutate(|v| {
                v.register_multisig(
                    "Regtest 2-of-3",
                    Network::Regtest,
                    policy.clone(),
                    Some(*id),
                    None,
                )
            })
            .unwrap();
    }
    let backup = Backup::new("Regtest", Network::Regtest, policy.clone()).unwrap();
    for descriptor in [&backup.receive_descriptor, &backup.change_descriptor] {
        let checked = node.rpc("getdescriptorinfo", json!([descriptor]));
        assert_eq!(
            checked["checksum"].as_str().unwrap(),
            descriptor.split_once('#').unwrap().1
        );
    }
    let addresses = node.rpc(
        "deriveaddresses",
        json!([backup.receive_descriptor, [0, 0]]),
    );
    assert_eq!(addresses[0], backup.address);
    let mut change = policy.clone();
    change.branch = 1;
    change.index = 5;
    let addresses = node.rpc("deriveaddresses", json!([backup.change_descriptor, [5, 5]]));
    assert_eq!(addresses[0], change.address(Network::Regtest).unwrap());
    let mut coordinator = Session::new(directory.path().join("coordinator.avvault"));
    coordinator.create(PASSWORD, Mode::Proxy).unwrap();
    coordinator
        .import_public(&signers[0].public_bytes().unwrap())
        .unwrap();
    let accounts = coordinator.snapshot().accounts;
    let blocks = node.rpc("generatetoaddress", json!([101, backup.address]));
    let block = node.rpc("getblock", json!([blocks[0], 2]));
    let previous = node
        .rpc(
            "getrawtransaction",
            json!([block["tx"][0]["txid"], false, blocks[0]]),
        )
        .as_str()
        .unwrap()
        .to_owned();
    let request = adapter::construct(
        Network::Regtest,
        vec![adapter::Utxo {
            transaction_hex: previous,
            vout: 0,
        }],
        vec![adapter::Recipient {
            address: accounts[0].address.clone(),
            satoshis: 100_000,
        }],
        &accounts[0].address,
        1000,
        &accounts,
    )
    .unwrap();
    coordinator
        .import_request(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let unsigned = coordinator.psbt_bytes(request.request_id).unwrap();
    save_sparrow_fixture("multi-unsigned.psbt", &unsigned);
    let decoded = node.rpc("decodepsbt", json!([STANDARD.encode(&unsigned)]));
    assert_eq!(
        decoded["outputs"][1]["bip32_derivs"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(decoded["outputs"][1]["witness_script"]["hex"]
        .as_str()
        .is_some());

    for (round, signer) in signers.iter_mut().take(2).enumerate() {
        let id = signer.import_psbt(&unsigned, Network::Regtest).unwrap();
        let review = signer.review(id).unwrap();
        signer
            .sign_multisig(review.token, true, PASSWORD, &HashMap::new(), &[])
            .unwrap();
        let partial = signer.psbt_bytes(id).unwrap();
        coordinator.import_psbt(&partial, Network::Regtest).unwrap();
        if round == 0 {
            assert!(coordinator.snapshot().responses.is_empty());
            let finalization = node.rpc("finalizepsbt", json!([STANDARD.encode(partial)]));
            assert_eq!(finalization["complete"], false);
        }
    }
    let snapshot = coordinator.snapshot();
    let response = &snapshot.responses[0];
    // Bitcoin Core independently finalizes the same standard PSBT.
    let finalized = node.rpc(
        "finalizepsbt",
        json!([STANDARD.encode(coordinator.psbt_bytes(request.request_id).unwrap())]),
    );
    assert_eq!(finalized["complete"], true);
    let raw = hex::encode(response.payload.decode().unwrap());
    assert_eq!(finalized["hex"], raw);
    assert_eq!(
        coordinator.transaction_bytes(request.request_id).unwrap(),
        response.payload.decode().unwrap()
    );
    save_sparrow_fixture(
        "multi-core-signed.psbt",
        &coordinator.psbt_bytes(request.request_id).unwrap(),
    );
    save_sparrow_fixture("multi-core-signed.txn", raw.as_bytes());
    let accepted = node.rpc("testmempoolaccept", json!([[raw]]));
    assert_eq!(accepted[0]["allowed"], true, "{accepted}");
    let txid = node.rpc("sendrawtransaction", json!([raw]));
    assert_eq!(txid, response.txid);
    let mined = node.rpc("generatetoaddress", json!([1, backup.address]));
    let confirmed = node.rpc("getrawtransaction", json!([txid, true, mined[0]]));
    assert_eq!(confirmed["confirmations"], 1);
    println!(
        "Bitcoin Core independently derived, finalized, accepted and confirmed 2-of-3 multisig: {}",
        response.txid
    );
}

fn save_sparrow_fixture(name: &str, bytes: &[u8]) {
    if let Ok(directory) = std::env::var("AIRVAULT_SPARROW_FIXTURES") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(name), bytes).unwrap();
    }
}
