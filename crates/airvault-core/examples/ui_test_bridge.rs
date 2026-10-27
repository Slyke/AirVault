//! Test process only. Not linked by the desktop executable. Public/disposable fixtures only.
use airvault_core::{
    bitcoin as adapter, capabilities,
    model::{Mode, Network},
    protocol::Request,
    session::Session,
    Error, Result,
};
use bitcoin::{
    absolute, consensus, transaction, Address, Amount, OutPoint, ScriptBuf, Sequence, Transaction,
    TxIn, TxOut, Witness,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    str::FromStr,
};
use uuid::Uuid;
fn text<'a>(a: &'a Value, name: &str) -> &'a str {
    a[name].as_str().unwrap_or("")
}
fn id(a: &Value, name: &str) -> Result<Uuid> {
    Uuid::parse_str(text(a, name)).map_err(|_| Error::Invalid("test UUID"))
}
fn handle(s: &mut Session, request: &Value, directory: &std::path::Path) -> Result<Value> {
    let a = &request["args"];
    s.tick();
    match text(request, "command") {
        "app_info" => {
            let binary = airvault_core::integrity::current_binary()?;
            Ok(
                json!({"version":env!("CARGO_PKG_VERSION"),"buildHash":"browser-test-fixture","sourceHash":"1234567890abcdef".repeat(4),"builtAt":1792022400u64,"target":"browser-native-test","dirty":true,"binaryPath":binary.path,"binarySha256":binary.sha256}),
            )
        }
        "plugin:dialog|save" => Ok(json!(directory.join("bitcoin-whitepaper.pdf"))),
        "save_whitepaper" => {
            let path = std::path::PathBuf::from(text(a, "path"));
            if path != directory.join("bitcoin-whitepaper.pdf") {
                return Err(Error::Invalid("test export path"));
            }
            airvault_core::storage::atomic_write(
                &path,
                include_bytes!("../../../public/reference/bitcoin-whitepaper.pdf"),
            )?;
            Ok(Value::Null)
        }
        "test_saved_whitepaper" => Ok(json!(airvault_core::integrity::file_sha256(
            &directory.join("bitcoin-whitepaper.pdf")
        )?)),
        "test_inspector_file" => {
            let data = if a["hex"].is_string() {
                hex::decode(text(a, "hex")).map_err(|_| Error::Invalid("test hex"))?
            } else {
                text(a, "text").as_bytes().to_vec()
            };
            let path = directory.join("inspection-input.dat");
            airvault_core::storage::atomic_write(&path, &data)?;
            Ok(json!(path))
        }
        "inspect_text" => Ok(serde_json::to_value(airvault_core::inspector::inspect(
            text(a, "text").as_bytes(),
            text(a, "hint"),
        )?)?),
        "inspect_file" => {
            let path = std::path::PathBuf::from(text(a, "path"));
            if !path.starts_with(directory) {
                return Err(Error::Invalid("test inspection path"));
            }
            let bytes = airvault_core::storage::read_bounded(
                &path,
                airvault_core::inspector::MAX_FILE_BYTES as u64,
            )?;
            Ok(serde_json::to_value(airvault_core::inspector::inspect(
                &bytes,
                text(a, "hint"),
            )?)?)
        }
        "inspect_exchange" => {
            let request_id = id(a, "id")?;
            let bytes = match text(a, "kind") {
                "psbt" => s.psbt_bytes(request_id)?,
                "request" => serde_json::to_vec_pretty(
                    s.vault()?
                        .data
                        .pending
                        .iter()
                        .find(|r| r.request_id == request_id)
                        .ok_or(Error::NotFound)?,
                )?,
                "response" => serde_json::to_vec_pretty(
                    s.vault()?
                        .data
                        .responses
                        .iter()
                        .find(|r| r.request_id == request_id)
                        .ok_or(Error::NotFound)?,
                )?,
                _ => return Err(Error::Invalid("test inspection source")),
            };
            Ok(serde_json::to_value(airvault_core::inspector::inspect(
                &bytes, "auto",
            )?)?)
        }
        "snapshot" => Ok(serde_json::to_value(s.snapshot())?),
        "capabilities" => Ok(serde_json::to_value(capabilities::registry())?),
        "cancel_ceremony" => {
            s.cancel_ceremony();
            Ok(Value::Null)
        }
        "activity" => {
            s.touch();
            Ok(Value::Null)
        }
        "create_vault" => {
            s.create(
                text(a, "password"),
                serde_json::from_value(a["mode"].clone())?,
            )?;
            Ok(Value::Null)
        }
        "unlock_vault" => {
            s.unlock(text(a, "password"))?;
            Ok(Value::Null)
        }
        "lock_vault" => {
            s.lock();
            Ok(Value::Null)
        }
        "generate_phrase" => Ok(serde_json::to_value(s.generate(
            a["words"].as_u64().unwrap_or(24) as u8,
            text(a, "prefix"),
            a["bits"].as_str(),
        )?)?),
        "confirm_phrase" => Ok(json!(s.confirm_phrase(
            id(a, "token")?,
            text(a, "confirmation"),
            text(a, "label"),
            a["keyPassword"].as_str()
        )?)),
        "import_phrase" => Ok(json!(s.mutate(|v| v.import_phrase(
            text(a, "label"),
            text(a, "phrase"),
            a["keyPassword"].as_str()
        ))?)),
        "delete_account" => {
            s.delete_account(
                id(a, "accountId")?,
                a["confirmed"].as_bool().unwrap_or(false),
            )?;
            Ok(Value::Null)
        }
        "verify_recovery" => Ok(json!(s.vault()?.verify_recovery(
            id(a, "rootId")?,
            text(a, "phrase"),
            a["keyPassword"].as_str()
        )?)),
        "add_identity" => Ok(json!(s.mutate(|v| v.add_identity(
            id(a, "rootId")?,
            text(a, "label"),
            text(a, "passphrase"),
            a["keyPassword"].as_str()
        ))?)),
        "derive_account" => Ok(json!(s.mutate(|v| {
            let account = v.derive_account(
                id(a, "identityId")?,
                text(a, "label"),
                serde_json::from_value(a["network"].clone())?,
                text(a, "path"),
                a["keyPassword"].as_str(),
            )?;
            if !a["token"].is_null() {
                v.add_token(account, serde_json::from_value(a["token"].clone())?)?;
            }
            Ok(account)
        })?)),
        "add_account_token" => {
            s.mutate(|v| {
                v.add_token(
                    id(a, "accountId")?,
                    serde_json::from_value(a["token"].clone())?,
                )
            })?;
            Ok(Value::Null)
        }
        "prepare_chain_transfer" => {
            if !s.vault()?.data.settings.mode.can_network() {
                return Err(Error::Mode);
            }
            let account = s
                .snapshot()
                .accounts
                .into_iter()
                .find(|item| Some(item.id) == id(a, "accountId").ok())
                .ok_or(Error::NotFound)?;
            let transaction = airvault_core::chains::transactions::Transaction {
                sender: account.address,
                public_key: account.public_key,
                operation: serde_json::from_value(a["operation"].clone())?,
            };
            let request =
                airvault_core::chains::transactions::request(account.network, transaction)?;
            Ok(json!(s.import_request(&serde_json::to_vec(&request)?)?))
        }
        "save_settings" => {
            s.settings(
                serde_json::from_value(a["settings"].clone())?,
                text(a, "password"),
                a["confirmed"].as_bool().unwrap_or(false),
            )?;
            Ok(Value::Null)
        }
        "review_request" => Ok(serde_json::to_value(s.review(id(a, "id")?)?)?),
        "approve_signing" => {
            let passwords: HashMap<Uuid, String> =
                serde_json::from_value(a["keyPasswords"].clone())?;
            let keys = passwords
                .into_iter()
                .map(|(k, v)| (k, zeroize::Zeroizing::new(v)))
                .collect();
            Ok(serde_json::to_value(s.sign(
                id(a, "token")?,
                a["confirmed"].as_bool().unwrap_or(false),
                text(a, "password"),
                &keys,
                &serde_json::from_value::<Vec<Uuid>>(a["selectedAccounts"].clone())?,
            )?)?)
        }

        "multisig_cosigner" => Ok(json!(s
            .vault()?
            .multisig_cosigner(
                id(a, "identityId")?,
                serde_json::from_value(a["network"].clone())?,
                a["account"].as_u64().unwrap_or(0) as u32,
                a["keyPassword"].as_str()
            )?
            .text())),
        "preview_multisig" => {
            let policy = airvault_core::multisig::Policy {
                threshold: a["threshold"].as_u64().unwrap_or(0) as u8,
                branch: a["branch"].as_u64().unwrap_or(0) as u32,
                index: a["index"].as_u64().unwrap_or(0) as u32,
                cosigners: text(a, "cosigners")
                    .lines()
                    .filter(|s| !s.trim().is_empty())
                    .map(airvault_core::multisig::Cosigner::parse)
                    .collect::<Result<Vec<_>>>()?,
            };
            Ok(serde_json::to_value(airvault_core::multisig::Backup::new(
                text(a, "label"),
                serde_json::from_value(a["network"].clone())?,
                policy,
            )?)?)
        }
        "register_multisig" => {
            if a["confirmed"] != true {
                return Err(Error::Approval);
            }
            let backup =
                airvault_core::multisig::Backup::parse(&serde_json::to_vec(&a["backup"])?)?;
            let identity = a["identityId"]
                .as_str()
                .map(|s| Uuid::parse_str(s).map_err(|_| Error::NotFound))
                .transpose()?;
            Ok(json!(s.mutate(|v| v.register_multisig(
                &backup.label,
                backup.network,
                backup.policy,
                identity,
                a["keyPassword"].as_str()
            ))?))
        }
        "approve_multisig" => {
            let passwords: HashMap<Uuid, String> =
                serde_json::from_value(a["keyPasswords"].clone())?;
            let keys = passwords
                .into_iter()
                .map(|(k, v)| (k, zeroize::Zeroizing::new(v)))
                .collect();
            Ok(json!(s.sign_multisig(
                id(a, "token")?,
                a["confirmed"] == true,
                text(a, "password"),
                &keys,
                &serde_json::from_value::<Vec<Uuid>>(a["selectedAccounts"].clone())?
            )?))
        }
        "export_multisig" => {
            let account = s
                .vault()?
                .data
                .accounts
                .iter()
                .find(|v| v.id.to_string() == text(a, "id"))
                .ok_or(Error::NotFound)?;
            let backup = airvault_core::multisig::Backup::new(
                &account.label,
                account.network,
                account.multisig.clone().ok_or(Error::NotFound)?,
            )?;
            airvault_core::storage::atomic_write(
                &directory.join("configuration.avmulti"),
                &serde_json::to_vec_pretty(&backup)?,
            )?;
            Ok(Value::Null)
        }
        "read_multisig" | "test_saved_multisig" => Ok(serde_json::to_value(
            airvault_core::multisig::Backup::parse(
                &std::fs::read(directory.join("configuration.avmulti"))
                    .map_err(|_| Error::NotFound)?,
            )?,
        )?),
        "plugin:dialog|open" => Ok(json!(directory.join(
            if directory.join("inspection-input.dat").exists() {
                "inspection-input.dat"
            } else {
                "configuration.avmulti"
            }
        ))),
        "export_psbt" => {
            airvault_core::storage::atomic_write(
                &directory.join("request.psbt"),
                &s.psbt_bytes(id(a, "id")?)?,
            )?;
            Ok(Value::Null)
        }
        "export_transaction" => {
            airvault_core::storage::atomic_write(
                &directory.join("signed.txn"),
                hex::encode(s.transaction_bytes(id(a, "id")?)?).as_bytes(),
            )?;
            Ok(Value::Null)
        }
        "test_saved_transaction" => {
            let raw = hex::decode(std::fs::read(directory.join("signed.txn"))?)
                .map_err(|_| Error::Verification)?;
            let tx: Transaction = consensus::deserialize(&raw).map_err(|_| Error::Verification)?;
            Ok(
                json!({"txid":tx.compute_txid().to_string(),"witnesses":tx.input.iter().map(|i|i.witness.len()).collect::<Vec<_>>()}),
            )
        }
        "test_saved_psbt" => {
            let psbt = airvault_core::multisig_psbt::decode(
                &std::fs::read(directory.join("request.psbt")).map_err(|_| Error::NotFound)?,
            )?;
            Ok(json!(psbt
                .inputs
                .iter()
                .map(|i| i.partial_sigs.len())
                .collect::<Vec<_>>()))
        }
        "test_multisig_seed" => {
            s.create("public-test-password", Mode::Offline)?;
            let mut cosigners = vec![];
            let mut identity = Uuid::nil();
            for n in 0..3 {
                let phrase = bip39::Mnemonic::from_entropy(&[n as u8; 16])
                    .unwrap()
                    .to_string();
                if n == 0 {
                    identity =
                        s.mutate(|v| v.import_phrase("Local disposable signer", &phrase, None))?;
                    cosigners.push(
                        s.vault()?
                            .multisig_cosigner(identity, Network::Regtest, 0, None)?
                            .text(),
                    );
                } else {
                    let mut other = Session::new(directory.join(format!("other-{n}.avvault")));
                    other.create("public-test-password", Mode::Offline)?;
                    let id = other
                        .mutate(|v| v.import_phrase("Other disposable signer", &phrase, None))?;
                    cosigners.push(
                        other
                            .vault()?
                            .multisig_cosigner(id, Network::Regtest, 0, None)?
                            .text(),
                    );
                }
            }
            Ok(json!({"cosigners":cosigners,"identity":identity}))
        }
        "test_fund" => {
            let accounts = s.snapshot().accounts;
            let account = accounts.first().ok_or(Error::NotFound)?;
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
            let r: Request = adapter::construct(
                account.network,
                vec![adapter::Utxo {
                    transaction_hex: hex::encode(consensus::serialize(&previous)),
                    vout: 0,
                }],
                vec![adapter::Recipient {
                    address: account.address.clone(),
                    satoshis: 40_000,
                }],
                &account.address,
                1000,
                &accounts,
            )?;
            s.import_request(&serde_json::to_vec(&r)?)?;
            Ok(json!(r.request_id))
        }
        "test_seed" => {
            s.create("public-test-password", Mode::Offline)?;
            s.mutate(|v| {
                let identity = v.import_phrase("Disposable public fixture", "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about", None)?;
                v.derive_account(identity, "Public testnet account", Network::Testnet, "m/84'/1'/0'/0/0", None)?;
                Ok(())
            })?;
            Ok(Value::Null)
        }
        _ => Err(Error::Unsupported("test bridge command")),
    }
}
fn main() {
    let mut directory = tempfile::tempdir().unwrap();
    let mut session = Session::new(directory.path().join("test.avvault"));
    for line in io::stdin().lock().lines() {
        let request: Value = match serde_json::from_str(&line.unwrap()) {
            Ok(v) => v,
            Err(_) => {
                println!(
                    "{}",
                    r#"{"ok":false,"error":{"message":"Invalid test request"}}"#
                );
                continue;
            }
        };
        let value = if request["command"] == "test_reset" {
            session.lock();
            directory = tempfile::tempdir().unwrap();
            session = Session::new(directory.path().join("test.avvault"));
            Ok(Value::Null)
        } else {
            handle(&mut session, &request, directory.path())
        };
        let reply = match value {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(error) => json!({"ok":false,"error":error}),
        };
        println!("{}", reply);
        io::stdout().flush().unwrap();
    }
}
