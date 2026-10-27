#![cfg(feature = "network")]
use airvault_core::{
    model::{Mode, Network, Settings, Token, VaultData},
    providers,
    storage::OpenVault,
};
use parity_scale_codec::Encode;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
const PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
struct Fixture {
    path: String,
    method: Option<String>,
    response: Value,
}
fn rest(path: &str, response: Value) -> Fixture {
    Fixture {
        path: path.into(),
        method: None,
        response,
    }
}
fn rpc(method: &str, result: Value) -> Fixture {
    Fixture {
        path: "/".into(),
        method: Some(method.into()),
        response: json!({"jsonrpc":"2.0","id":1,"result":result,"error":null}),
    }
}
struct Server {
    url: String,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(fixtures: Vec<Fixture>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let worker = thread::spawn(move || {
            for fixture in fixtures {
                let deadline = Instant::now() + Duration::from_secs(8);
                let mut stream = loop {
                    if let Ok((s, _)) = listener.accept() {
                        break s;
                    }
                    assert!(Instant::now() < deadline, "expected {}", fixture.path);
                    thread::sleep(Duration::from_millis(5));
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                assert_eq!(first.split_whitespace().nth(1).unwrap(), fixture.path);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':') {
                        if key.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse::<usize>().unwrap();
                        }
                    }
                }
                assert!(length < 4_000_000);
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                if let Some(method) = fixture.method {
                    assert_eq!(
                        serde_json::from_slice::<Value>(&body).unwrap()["method"],
                        method
                    );
                }
                let data = fixture.response.to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",data.len(),data).unwrap();
            }
        });
        Self {
            url,
            worker: Some(worker),
        }
    }
    fn done(mut self) {
        self.worker.take().unwrap().join().unwrap();
    }
}
fn settings(network: Network, url: &str) -> Settings {
    let mut settings = Settings::default();
    settings.mode = Mode::Combined;
    settings.providers.insert(
        serde_json::to_value(network)
            .unwrap()
            .as_str()
            .unwrap()
            .into(),
        url.into(),
    );
    settings
}
#[test]
fn provider_state_and_balance_responses_are_checked_and_normalized() {
    let mut vault = OpenVault::create("123456", VaultData::default()).unwrap();
    let identity = vault.import_phrase("Fixture", PHRASE, None).unwrap();
    for network in [
        Network::Ethereum,
        Network::SolanaDevnet,
        Network::Cosmos,
        Network::Xrp,
        Network::TronNile,
        Network::CardanoPreprod,
        Network::Litecoin,
        Network::Dogecoin,
        Network::BitcoinCash,
        Network::Polkadot,
    ] {
        let id = vault
            .derive_account(
                identity,
                network.chain().name,
                network,
                network.chain().path,
                None,
            )
            .unwrap();
        let mut account = vault
            .data
            .accounts
            .iter()
            .find(|a| a.id == id)
            .unwrap()
            .clone();
        let address = &account.address;
        let (mut fixtures, expected) = match network {
            Network::Ethereum => (
                vec![
                    rpc("eth_chainId", json!("0x1")),
                    rpc("eth_getBalance", json!("0xde0b6b3a7640000")),
                ],
                "1",
            ),
            Network::SolanaDevnet => (
                vec![
                    rpc(
                        "getGenesisHash",
                        json!("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"),
                    ),
                    rpc("getBalance", json!({"value":1000000000})),
                ],
                "1",
            ),
            Network::Cosmos => (
                vec![
                    rest(
                        "/cosmos/base/tendermint/v1beta1/node_info",
                        json!({"default_node_info":{"network":"cosmoshub-4"}}),
                    ),
                    rest(
                        &format!("/cosmos/bank/v1beta1/balances/{address}"),
                        json!({"balances":[{"denom":"uatom","amount":"1000000"}]}),
                    ),
                ],
                "1",
            ),
            Network::Xrp => (
                vec![
                    rpc("server_info", json!({"info":{"network_id":0}})),
                    rpc(
                        "account_info",
                        json!({"account_data":{"Balance":"1000000"}}),
                    ),
                ],
                "1",
            ),
            Network::TronNile => (
                vec![
                    rest(
                        "/wallet/getblockbynum",
                        json!({"blockID":format!("{}cd8690dc","00".repeat(28))}),
                    ),
                    rest("/wallet/getaccount", json!({"balance":1000000})),
                ],
                "1",
            ),
            Network::CardanoPreprod => (
                vec![
                    rest("/genesis", json!([{"networkmagic":1}])),
                    rest("/address_info", json!([{"balance":"1000000"}])),
                ],
                "1",
            ),
            Network::Polkadot => (
                vec![
                    rpc(
                        "chain_getBlockHash",
                        json!(format!(
                            "0x{}",
                            airvault_core::chains::transactions::polkadot::GENESIS
                        )),
                    ),
                    rpc("state_getStorage", Value::Null),
                ],
                "0",
            ),
            _ => (
                vec![
                    rest(
                        "/",
                        json!({"blockbook":{"coin":network.chain().name},"backend":{"chain":"main"}}),
                    ),
                    rest(
                        &format!("/address/{address}"),
                        json!({"balance":"100000000"}),
                    ),
                ],
                "1",
            ),
        };
        if matches!(
            network,
            Network::Ethereum | Network::SolanaDevnet | Network::TronNile
        ) {
            account.tokens.push(Token {
                standard: network.chain().tokens.to_lowercase().replace('-', ""),
                contract: address.clone(),
                symbol: "PUBLIC".into(),
                decimals: 6,
            });
            fixtures.push(match network{
                Network::Ethereum=>rpc("eth_call",json!("0xf4240")),
                Network::SolanaDevnet=>rpc("getAccountInfo",json!({"value":{"data":{"parsed":{"info":{"mint":address,"tokenAmount":{"amount":"1000000","decimals":6}}}}}})),
                _=>rest("/wallet/triggerconstantcontract",json!({"result":{"result":true},"constant_result":[format!("{:064x}",1000000)]})),
            });
        }
        let server = Server::new(fixtures);
        let result = providers::balance(&settings(network, &server.url), &account)
            .unwrap_or_else(|e| panic!("{} balance: {e}", network.chain().name));
        assert_eq!(result["balance"], expected);
        if !account.tokens.is_empty() {
            assert_eq!(result["assets"][0]["balance"], "1");
        }
        server.done();
        let chain = match network {
            Network::Ethereum => rpc("eth_chainId", json!("0x1")),
            Network::SolanaDevnet => rpc(
                "getGenesisHash",
                json!("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"),
            ),
            Network::Cosmos => rest(
                "/cosmos/base/tendermint/v1beta1/node_info",
                json!({"default_node_info":{"network":"cosmoshub-4"}}),
            ),
            Network::Xrp => rpc("server_info", json!({"info":{"network_id":0}})),
            Network::TronNile => rest(
                "/wallet/getblockbynum",
                json!({"blockID":format!("{}cd8690dc","00".repeat(28))}),
            ),
            Network::CardanoPreprod => rest("/genesis", json!([{"networkmagic":1}])),
            Network::Polkadot => rpc(
                "chain_getBlockHash",
                json!(format!(
                    "0x{}",
                    airvault_core::chains::transactions::polkadot::GENESIS
                )),
            ),
            _ => rest(
                "/",
                json!({"blockbook":{"coin":network.chain().name},"backend":{"chain":"main"}}),
            ),
        };
        let mut fixtures = vec![chain];
        fixtures.extend(match network {
            Network::Ethereum=>vec![rpc("eth_getTransactionCount",json!("0x2")),rpc("eth_gasPrice",json!("0x3b9aca00"))],
            Network::SolanaDevnet=>vec![rpc("getLatestBlockhash",json!({"value":{"blockhash":bs58::encode([5;32]).into_string()}}))],
            Network::Cosmos=>vec![rest(&format!("/cosmos/auth/v1beta1/accounts/{address}"),json!({"account":{"base_vesting_account":{"base_account":{"account_number":"1","sequence":"2"}}}}))],
            Network::Xrp=>vec![rpc("account_info",json!({"account_data":{"Sequence":2}})),rpc("fee",json!({"ledger_current_index":100,"drops":{"open_ledger_fee":"12"}}))],
            Network::TronNile=>vec![rest("/wallet/getnowblock",json!({"blockID":hex::encode([3;32]),"block_header":{"raw_data":{"number":65538,"timestamp":1700000000000u64}}}))],
            Network::CardanoPreprod=>vec![rest("/tip",json!([{"abs_slot":1000000}])),rest("/address_info",json!([{"utxo_set":[]}]))],
            Network::Polkadot=>vec![rpc("state_getRuntimeVersion",json!({"specVersion":2005000,"transactionVersion":15,"specName":"statemint"})),rpc("system_accountNextIndex",json!(2)),rpc("state_call",json!(format!("0x{}",hex::encode(Some(include_bytes!("fixtures/polkadot-hub-2005000.scale").to_vec()).encode()))))],
            _=>vec![rest(&format!("/utxo/{address}"),json!([]))],
        });
        let server = Server::new(fixtures);
        let result = providers::fetch_state(&settings(network, &server.url), &account)
            .unwrap_or_else(|e| panic!("{} state: {e}", network.chain().name));
        assert!(result.as_object().is_some_and(|v| !v.is_empty()));
        server.done();
        let mut offline = settings(network, "https://must-not-resolve.invalid");
        offline.mode = Mode::Offline;
        assert!(matches!(
            providers::balance(&offline, &account),
            Err(airvault_core::Error::Mode)
        ));
        assert!(matches!(
            providers::fetch_state(&offline, &account),
            Err(airvault_core::Error::Mode)
        ));
    }
}
#[test]
fn wrong_chain_and_remote_plaintext_providers_are_rejected() {
    let server = Server::new(vec![rpc("eth_chainId", json!("0xaa36a7"))]);
    assert!(providers::verify_chain(Mode::Combined, &server.url, Network::Ethereum).is_err());
    server.done();
    assert!(
        providers::verify_chain(Mode::Combined, "http://remote.invalid", Network::Ethereum)
            .is_err()
    );
    assert!(providers::verify_chain(
        Mode::Offline,
        "https://must-not-resolve.invalid",
        Network::Ethereum
    )
    .is_err());
}
