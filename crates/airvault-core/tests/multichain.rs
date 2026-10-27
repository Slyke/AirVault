use airvault_core::{
    chains::{
        self,
        transactions::{self as tx, Operation, Transaction},
    },
    model::{Mode, Network},
    session::Session,
};
use bitcoin::{absolute, consensus, transaction, Amount, Transaction as BtcTx, TxOut};
use std::collections::HashMap;
const PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
fn operation(network: Network, sender: &str, to: &str) -> Operation {
    if network.evm_id().is_some() {
        return Operation::Evm(tx::evm::Transfer {
            to: to.into(),
            amount: "1000000000000000".into(),
            nonce: 0,
            gas_limit: 21000,
            gas_price: "1000000000".into(),
            asset: None,
        });
    }
    match network {
        Network::Solana | Network::SolanaDevnet => Operation::Solana(tx::solana::Transfer {
            to: to.into(),
            amount: "1000".into(),
            recent_blockhash: bs58::encode([8; 32]).into_string(),
            asset: None,
        }),
        Network::Cosmos => Operation::Cosmos(tx::cosmos::Transfer {
            to: to.into(),
            amount: "1000000".into(),
            account_number: 42,
            sequence: 7,
            gas_limit: 200000,
            fee: "5000".into(),
            memo: "AirVault vector".into(),
            action: Default::default(),
        }),
        Network::Xrp => Operation::Xrp(tx::xrp::Transfer {
            to: to.into(),
            amount: "1000000".into(),
            fee: "12".into(),
            sequence: 1,
            last_ledger_sequence: 50000000,
            destination_tag: Some(0),
        }),
        Network::Tron | Network::TronNile => Operation::Tron(tx::tron::Transfer {
            to: to.into(),
            amount: "1000000".into(),
            ref_block_bytes: "1234".into(),
            ref_block_hash: "1234567890abcdef".into(),
            timestamp: 1700000000000,
            expiration: 1700000060000,
            fee_limit: 0,
            asset: None,
        }),
        Network::Dogecoin | Network::Litecoin | Network::BitcoinCash => {
            let prev = BtcTx {
                version: transaction::Version::TWO,
                lock_time: absolute::LockTime::ZERO,
                input: vec![],
                output: vec![TxOut {
                    value: Amount::from_sat(100_000_000),
                    script_pubkey: tx::utxo::script(sender, network).unwrap(),
                }],
            };
            Operation::Utxo(tx::utxo::Transfer {
                to: to.into(),
                amount: "50000000".into(),
                fee: "1000000".into(),
                utxos: vec![airvault_core::bitcoin::Utxo {
                    transaction_hex: hex::encode(consensus::serialize(&prev)),
                    vout: 0,
                }],
            })
        }
        Network::Cardano | Network::CardanoPreprod => {
            use cardano_serialization_lib as ada;
            let mut outputs = ada::TransactionOutputs::new();
            outputs.add(&ada::TransactionOutput::new(
                &ada::Address::from_bech32(sender).unwrap(),
                &ada::Value::new(&ada::BigNum::from_str("10000000").unwrap()),
            ));
            let prev = ada::Transaction::new(
                &ada::TransactionBody::new_tx_body(
                    &ada::TransactionInputs::new(),
                    &outputs,
                    &ada::BigNum::from_str("0").unwrap(),
                ),
                &ada::TransactionWitnessSet::new(),
                None,
            );
            Operation::Cardano(tx::cardano::Transfer {
                to: to.into(),
                amount: "3000000".into(),
                fee: "200000".into(),
                invalid_after: 100000000,
                utxos: vec![tx::cardano::Utxo {
                    transaction_cbor: hex::encode(prev.to_bytes()),
                    index: 0,
                }],
            })
        }
        Network::Polkadot => Operation::Polkadot(tx::polkadot::Transfer {
            to: to.into(),
            amount: "1000000000".into(),
            nonce: 1,
            tip: "0".into(),
            spec_version: 2005000,
            transaction_version: 15,
            spec_name: "statemint".into(),
            metadata: hex::encode(include_bytes!("fixtures/polkadot-hub-2005000.scale")),
        }),
        _ => panic!("fixture chain"),
    }
}
#[test]
fn every_chain_requires_approval_and_verifies_native_signatures() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::new(dir.path().join("vault.avvault"));
    session.create("123456", Mode::Offline).unwrap();
    let identity = session
        .mutate(|v| v.import_phrase("Public vectors", PHRASE, None))
        .unwrap();
    for chain in chains::registry()
        .into_iter()
        .filter(|c| !c.network.is_bitcoin())
    {
        let id = session
            .mutate(|v| v.derive_account(identity, chain.name, chain.network, chain.path, None))
            .unwrap();
        let account = session
            .snapshot()
            .accounts
            .into_iter()
            .find(|a| a.id == id)
            .unwrap();
        let mut other_path = chain.path.to_string();
        if chain.network == Network::Polkadot {
            other_path = "//1".into();
        } else if other_path.ends_with("0'") {
            other_path.truncate(other_path.len() - 2);
            other_path.push_str("1'");
        } else {
            other_path.pop();
            other_path.push('1');
        }
        let to = session
            .mutate(|v| {
                v.derive_account(
                    identity,
                    &format!("{} recipient", chain.name),
                    chain.network,
                    &other_path,
                    None,
                )
            })
            .unwrap();
        let target = session
            .snapshot()
            .accounts
            .into_iter()
            .find(|a| a.id == to)
            .unwrap();
        let request = tx::request(
            chain.network,
            Transaction {
                sender: account.address.clone(),
                public_key: account.public_key.clone(),
                operation: operation(chain.network, &account.address, &target.address),
            },
        )
        .unwrap_or_else(|e| panic!("{} prepare: {e}", chain.name));
        let id = session
            .import_request(&serde_json::to_vec(&request).unwrap())
            .unwrap();
        assert!(session
            .sign(uuid::Uuid::new_v4(), true, "123456", &HashMap::new(), &[])
            .is_err());
        let review = session.review(id).unwrap();
        assert!(review.inspection.chain.is_some());
        assert!(session
            .sign(review.token, false, "123456", &HashMap::new(), &[])
            .is_err());
        let review = session.review(id).unwrap();
        let response = session
            .sign(review.token, true, "123456", &HashMap::new(), &[])
            .unwrap_or_else(|e| panic!("{} sign: {e}", chain.name));
        tx::verify(&request, &response, &session.snapshot().accounts).unwrap();
        #[cfg(feature = "network")]
        {
            let proxy_dir = tempfile::tempdir().unwrap();
            let mut proxy = Session::new(proxy_dir.path().join("proxy.avvault"));
            proxy.create("123456", Mode::Proxy).unwrap();
            proxy
                .import_public(&session.public_bytes().unwrap())
                .unwrap();
            proxy
                .import_request(&serde_json::to_vec(&request).unwrap())
                .unwrap();
            proxy
                .import_response(&serde_json::to_vec(&response).unwrap())
                .unwrap();
            assert_eq!(proxy.snapshot().responses[0].txid, response.txid);
        }
        let mut tampered = response.clone();
        let mut bytes = tampered.payload.decode().unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 1;
        tampered.payload = airvault_core::protocol::Payload::new(&bytes);
        tampered.payload_sha256 = airvault_core::protocol::digest(&bytes);
        assert!(
            tx::verify(&request, &tampered, &session.snapshot().accounts).is_err(),
            "{}",
            chain.name
        );
        let mut swapped = request.clone();
        swapped.network = if chain.network == Network::Ethereum {
            Network::Sepolia
        } else {
            Network::Ethereum
        };
        assert!(tx::verify(&swapped, &response, &session.snapshot().accounts).is_err());
        println!(
            "{}: derived, approved, signed, verified, tampering rejected",
            chain.name
        );
    }
    // Every exported public address must be validated against its public keys.
    airvault_core::protocol::PublicBundle::parse(&session.public_bytes().unwrap()).unwrap();
}
#[test]
fn known_ethereum_address_and_six_character_password_policy() {
    use airvault_core::{model::VaultData, storage::OpenVault};
    assert!(OpenVault::create("12345", VaultData::default()).is_err());
    let mut vault = OpenVault::create("123456", VaultData::default()).unwrap();
    let id = vault.import_phrase("Vector", PHRASE, None).unwrap();
    vault
        .derive_account(
            id,
            "ETH",
            Network::Ethereum,
            Network::Ethereum.chain().path,
            None,
        )
        .unwrap();
    assert_eq!(
        vault.data.accounts[0].address,
        "0x9858EfFD232B4033E47d90003D41EC34EcaEda94"
    );
    assert!(OpenVault::create("🦀🦀🦀", VaultData::default()).is_err());
    assert!(OpenVault::create("🦀🦀🦀🦀🦀🦀", VaultData::default()).is_ok());
}

#[test]
fn addresses_match_independent_bip_utils_vectors() {
    use airvault_core::{model::VaultData, storage::OpenVault};
    let expected: HashMap<String, String> =
        serde_json::from_str(include_str!("fixtures/addresses.json")).unwrap();
    let mut vault = OpenVault::create("123456", VaultData::default()).unwrap();
    let identity = vault.import_phrase("Public vector", PHRASE, None).unwrap();
    for (name, address) in expected {
        let (network, path) = if name == "polkadot//1" {
            (Network::Polkadot, "//1".to_owned())
        } else {
            let network: Network = serde_json::from_value(serde_json::json!(name)).unwrap();
            (network, network.chain().path.to_owned())
        };
        let id = vault
            .derive_account(identity, &name, network, &path, None)
            .unwrap();
        assert_eq!(
            vault
                .data
                .accounts
                .iter()
                .find(|a| a.id == id)
                .unwrap()
                .address,
            address,
            "{name}"
        );
    }
}

#[test]
fn tokens_nfts_and_staking_sign_only_the_reviewed_operation() {
    use airvault_core::model::Token;
    let directory = tempfile::tempdir().unwrap();
    let mut session = Session::new(directory.path().join("vault.avvault"));
    session.create("123456", Mode::Offline).unwrap();
    let identity = session
        .mutate(|v| v.import_phrase("Public fixture", PHRASE, None))
        .unwrap();
    let mut cases = vec![];
    for network in [
        Network::Ethereum,
        Network::Solana,
        Network::Tron,
        Network::Cosmos,
    ] {
        let id = session
            .mutate(|v| {
                v.derive_account(
                    identity,
                    network.chain().name,
                    network,
                    network.chain().path,
                    None,
                )
            })
            .unwrap();
        let account = session
            .snapshot()
            .accounts
            .into_iter()
            .find(|a| a.id == id)
            .unwrap();
        let token = Token {
            standard: network.chain().tokens.to_lowercase().replace('-', ""),
            contract: if network == Network::Solana {
                bs58::encode([9; 32]).into_string()
            } else {
                account.address.clone()
            },
            symbol: "PUBLIC".into(),
            decimals: 6,
        };
        if network != Network::Cosmos {
            let duplicate = token.clone();
            session.mutate(|v| v.add_token(id, token)).unwrap();
            assert!(session.mutate(|v| v.add_token(id, duplicate)).is_err());
        }
        let mut operation = operation(network, &account.address, &account.address);
        match &mut operation {
            Operation::Evm(t) => {
                t.gas_limit = 100000;
                t.asset = Some(tx::evm::Asset::Erc20 {
                    contract: account.address.clone(),
                    symbol: "PUBLIC".into(),
                    decimals: 6,
                });
                cases.push((account.clone(), operation.clone()));
                for standard in ["erc721", "erc1155"] {
                    let asset = if standard == "erc721" {
                        tx::evm::Asset::Erc721 {
                            contract: account.address.clone(),
                            token_id: "42".into(),
                        }
                    } else {
                        tx::evm::Asset::Erc1155 {
                            contract: account.address.clone(),
                            token_id: "42".into(),
                        }
                    };
                    cases.push((
                        account.clone(),
                        Operation::Evm(tx::evm::Transfer {
                            asset: Some(asset),
                            amount: "1".into(),
                            gas_limit: 150000,
                            to: account.address.clone(),
                            nonce: 1,
                            gas_price: "1000000000".into(),
                        }),
                    ));
                }
            }
            Operation::Solana(t) => {
                t.to = bs58::encode([8; 32]).into_string();
                t.asset = Some(tx::solana::Token {
                    mint: bs58::encode([9; 32]).into_string(),
                    symbol: "PUBLIC".into(),
                    decimals: 6,
                });
                cases.push((account.clone(), operation));
            }
            Operation::Tron(t) => {
                t.fee_limit = 100000000;
                t.asset = Some(tx::tron::Token {
                    contract: account.address.clone(),
                    symbol: "PUBLIC".into(),
                    decimals: 6,
                });
                cases.push((account.clone(), operation));
            }
            Operation::Cosmos(t) => {
                t.to = bech32::encode::<bech32::Bech32>(
                    bech32::Hrp::parse("cosmosvaloper").unwrap(),
                    &[7; 20],
                )
                .unwrap();
                for action in [
                    tx::cosmos::Action::Delegate,
                    tx::cosmos::Action::Undelegate,
                    tx::cosmos::Action::ClaimRewards,
                ] {
                    t.action = action;
                    t.amount = if matches!(t.action, tx::cosmos::Action::ClaimRewards) {
                        "0"
                    } else {
                        "1000000"
                    }
                    .into();
                    cases.push((account.clone(), Operation::Cosmos(t.clone())));
                }
            }
            _ => unreachable!(),
        }
    }
    for (account, operation) in cases {
        let transaction = Transaction {
            sender: account.address.clone(),
            public_key: account.public_key.clone(),
            operation,
        };
        let request = tx::request(account.network, transaction.clone()).unwrap();
        let id = session
            .import_request(&serde_json::to_vec(&request).unwrap())
            .unwrap();
        let review = session.review(id).unwrap();
        let response = session
            .sign(review.token, true, "123456", &HashMap::new(), &[])
            .unwrap();
        tx::verify(&request, &response, &session.snapshot().accounts).unwrap();
        let mut tampered = transaction.clone();
        match &mut tampered.operation {
            Operation::Evm(t) => t.to = "0x1111111111111111111111111111111111111111".into(),
            Operation::Solana(t) => t.amount = "2".into(),
            Operation::Tron(t) => t.amount = "2".into(),
            Operation::Cosmos(t) => t.sequence += 1,
            _ => unreachable!(),
        }
        let changed = tx::request(account.network, tampered).unwrap();
        let mut forged = response.clone();
        forged.request_id = changed.request_id;
        forged.request_digest = changed.binding().unwrap();
        assert!(tx::verify(&changed, &forged, &session.snapshot().accounts).is_err());
        // Symbols and decimals on an incoming request cannot relabel an account's registered token.
        let mut mislabeled = transaction;
        let has_token = match &mut mislabeled.operation {
            Operation::Evm(tx::evm::Transfer {
                asset: Some(tx::evm::Asset::Erc20 { decimals, .. }),
                ..
            }) => {
                *decimals = 18;
                true
            }
            Operation::Solana(t) => {
                t.asset.as_mut().unwrap().decimals = 18;
                true
            }
            Operation::Tron(t) => {
                t.asset.as_mut().unwrap().decimals = 18;
                true
            }
            _ => false,
        };
        if has_token {
            let malicious = tx::request(account.network, mislabeled).unwrap();
            assert!(tx::inspect(&malicious, &session.snapshot().accounts).is_err());
        }
        session.lock();
        session.record_broadcast(airvault_core::model::History {
            id: uuid::Uuid::new_v4(),
            request_id: id,
            action: "broadcast".into(),
            at: 1,
            digest: request.binding().unwrap(),
            txid: Some(response.txid.clone()),
        });
        session.unlock("123456").unwrap();
        assert!(session
            .snapshot()
            .history
            .iter()
            .any(|h| h.action == "broadcast" && h.txid.as_ref() == Some(&response.txid)));
    }
}
