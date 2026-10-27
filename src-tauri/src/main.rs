#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(feature = "network")]
use airvault_core::{
    bitcoin::{self, Recipient, Utxo},
    model,
};
use airvault_core::{
    model::{Mode, Network, Settings},
    protocol,
    session::{Ceremony, Review, Session, Snapshot},
    storage, Error, Result,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Manager, State};
use uuid::Uuid;
use zeroize::Zeroizing;

#[cfg(windows)]
mod power;

struct AppState {
    session: Arc<Mutex<Session>>,
    _lock: std::fs::File,
}
fn with<T>(state: &State<AppState>, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
    let mut session = state.session.lock().map_err(|_| Error::Locked)?;
    #[cfg(windows)]
    if power::take_lock_request() {
        session.lock();
    }
    session.tick();
    f(&mut session)
}
#[tauri::command]
async fn inspect_file(path: PathBuf, hint: String) -> Result<airvault_core::inspector::Report> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = storage::read_bounded(&path, airvault_core::inspector::MAX_FILE_BYTES as u64)?;
        airvault_core::inspector::inspect(&bytes, &hint)
    })
    .await
    .map_err(|_| Error::Invalid("Inspector task failed"))?
}
#[tauri::command]
async fn inspect_text(text: String, hint: String) -> Result<airvault_core::inspector::Report> {
    tauri::async_runtime::spawn_blocking(move || {
        airvault_core::inspector::inspect(text.as_bytes(), &hint)
    })
    .await
    .map_err(|_| Error::Invalid("Inspector task failed"))?
}
#[tauri::command]
fn inspect_exchange(
    state: State<AppState>,
    id: Uuid,
    kind: String,
) -> Result<airvault_core::inspector::Report> {
    with(&state, |s| {
        let bytes = match kind.as_str() {
            "psbt" => s.psbt_bytes(id)?,
            "request" => serde_json::to_vec_pretty(
                s.vault()?
                    .data
                    .pending
                    .iter()
                    .find(|r| r.request_id == id)
                    .ok_or(Error::NotFound)?,
            )?,
            "response" => serde_json::to_vec_pretty(
                s.vault()?
                    .data
                    .responses
                    .iter()
                    .find(|r| r.request_id == id)
                    .ok_or(Error::NotFound)?,
            )?,
            _ => return Err(Error::Invalid("Inspector exchange source")),
        };
        airvault_core::inspector::inspect(&bytes, "auto")
    })
}
#[tauri::command]
fn snapshot(state: State<AppState>) -> Result<Snapshot> {
    with(&state, |s| Ok(s.snapshot()))
}
#[tauri::command]
fn create_vault(state: State<AppState>, password: String, mode: Mode) -> Result<()> {
    let p = Zeroizing::new(password);
    with(&state, |s| s.create(&p, mode))
}
#[tauri::command]
fn unlock_vault(state: State<AppState>, password: String) -> Result<()> {
    let p = Zeroizing::new(password);
    with(&state, |s| s.unlock(&p))
}
#[tauri::command]
fn lock_vault(state: State<AppState>) -> Result<()> {
    with(&state, |s| {
        s.lock();
        Ok(())
    })
}
#[tauri::command]
fn activity(state: State<AppState>) -> Result<()> {
    with(&state, |s| {
        s.touch();
        Ok(())
    })
}
#[tauri::command]
fn cancel_ceremony(state: State<AppState>) -> Result<()> {
    with(&state, |s| {
        s.cancel_ceremony();
        Ok(())
    })
}
#[tauri::command]
fn generate_phrase(
    state: State<AppState>,
    words: u8,
    prefix: String,
    bits: Option<String>,
) -> Result<Ceremony> {
    let prefix = Zeroizing::new(prefix);
    let bits = bits.map(Zeroizing::new);
    with(&state, |s| {
        s.generate(words, &prefix, bits.as_ref().map(|b| b.as_str()))
    })
}
#[tauri::command]
fn confirm_phrase(
    state: State<AppState>,
    token: Uuid,
    confirmation: String,
    label: String,
    key_password: Option<String>,
) -> Result<Uuid> {
    let phrase = Zeroizing::new(confirmation);
    let password = key_password.map(Zeroizing::new);
    with(&state, |s| {
        s.confirm_phrase(
            token,
            &phrase,
            &label,
            password.as_ref().map(|p| p.as_str()),
        )
    })
}
#[tauri::command]
fn import_phrase(
    state: State<AppState>,
    label: String,
    phrase: String,
    key_password: Option<String>,
) -> Result<Uuid> {
    let phrase = Zeroizing::new(phrase);
    let password = key_password.map(Zeroizing::new);
    with(&state, |s| {
        s.mutate(|v| v.import_phrase(&label, &phrase, password.as_ref().map(|p| p.as_str())))
    })
}
#[tauri::command]
fn import_wif(
    state: State<AppState>,
    label: String,
    wif: String,
    network: Network,
    key_password: Option<String>,
) -> Result<Uuid> {
    let wif = Zeroizing::new(wif);
    let password = key_password.map(Zeroizing::new);
    with(&state, |s| {
        s.mutate(|v| v.import_wif(&label, &wif, network, password.as_ref().map(|p| p.as_str())))
    })
}
#[tauri::command]
fn add_identity(
    state: State<AppState>,
    root_id: Uuid,
    label: String,
    passphrase: String,
    key_password: Option<String>,
) -> Result<Uuid> {
    let passphrase = Zeroizing::new(passphrase);
    let password = key_password.map(Zeroizing::new);
    with(&state, |s| {
        s.mutate(|v| {
            v.add_identity(
                root_id,
                &label,
                &passphrase,
                password.as_ref().map(|p| p.as_str()),
            )
        })
    })
}
#[tauri::command]
fn derive_account(
    state: State<AppState>,
    identity_id: Uuid,
    label: String,
    network: Network,
    path: String,
    key_password: Option<String>,
    token: Option<model::Token>,
) -> Result<Uuid> {
    let password = key_password.map(Zeroizing::new);
    with(&state, |s| {
        s.mutate(|v| {
            if let Some(token) = &token {
                token.validate(network)?;
            }
            let id = v.derive_account(
                identity_id,
                &label,
                network,
                &path,
                password.as_ref().map(|p| p.as_str()),
            )?;
            if let Some(token) = token {
                v.data
                    .accounts
                    .iter_mut()
                    .find(|a| a.id == id)
                    .ok_or(Error::NotFound)?
                    .tokens
                    .push(token);
            }
            Ok(id)
        })
    })
}
#[tauri::command]
fn delete_account(state: State<AppState>, account_id: Uuid, confirmed: bool) -> Result<()> {
    with(&state, |s| s.delete_account(account_id, confirmed))
}
#[tauri::command]
fn add_account_token(state: State<AppState>, account_id: Uuid, token: model::Token) -> Result<()> {
    with(&state, |s| s.mutate(|v| v.add_token(account_id, token)))
}
#[tauri::command]
fn verify_recovery(
    state: State<AppState>,
    root_id: Uuid,
    phrase: String,
    key_password: Option<String>,
) -> Result<bool> {
    let phrase = Zeroizing::new(phrase);
    let password = key_password.map(Zeroizing::new);
    with(&state, |s| {
        s.vault()?
            .verify_recovery(root_id, &phrase, password.as_ref().map(|p| p.as_str()))
    })
}
#[tauri::command]
fn change_password(
    state: State<AppState>,
    old_password: String,
    new_password: String,
) -> Result<()> {
    let old = Zeroizing::new(old_password);
    let new = Zeroizing::new(new_password);
    with(&state, |s| s.mutate(|v| v.change_password(&old, &new)))
}
#[tauri::command]
fn save_settings(
    state: State<AppState>,
    settings: Settings,
    password: String,
    confirmed: bool,
) -> Result<()> {
    let password = Zeroizing::new(password);
    with(&state, |s| s.settings(settings, &password, confirmed))
}
fn extension(path: &std::path::Path, expected: &str) -> Result<()> {
    if path.extension().and_then(|s| s.to_str()) != Some(expected) {
        return Err(Error::Invalid("incorrect file extension"));
    }
    Ok(())
}
#[tauri::command]
fn import_file(state: State<AppState>, path: PathBuf) -> Result<String> {
    with(&state, |s| {
        let bytes = storage::read_bounded(&path, protocol::MAX_EXCHANGE_BYTES as u64)?;
        match path.extension().and_then(|s| s.to_str()) {
            Some("avpub") => {
                s.import_public(&bytes)?;
                Ok("public".into())
            }
            Some("avreq") => {
                s.import_request(&bytes)?;
                Ok("request".into())
            }
            Some("avsig") => {
                s.import_response(&bytes)?;
                Ok("verified".into())
            }
            _ => Err(Error::Unsupported(
                "choose an .avpub, .avreq or .avsig file",
            )),
        }
    })
}
#[tauri::command]
fn export_public(state: State<AppState>, path: PathBuf) -> Result<()> {
    extension(&path, "avpub")?;
    with(&state, |s| storage::atomic_write(&path, &s.public_bytes()?))
}
#[tauri::command]
fn export_request(state: State<AppState>, path: PathBuf, id: Uuid) -> Result<()> {
    extension(&path, "avreq")?;
    with(&state, |s| {
        let r = s
            .vault()?
            .data
            .pending
            .iter()
            .find(|r| r.request_id == id)
            .ok_or(Error::NotFound)?;
        storage::atomic_write(&path, &serde_json::to_vec_pretty(r)?)
    })
}
#[tauri::command]
fn export_response(state: State<AppState>, path: PathBuf, id: Uuid) -> Result<()> {
    extension(&path, "avsig")?;
    with(&state, |s| {
        let r = s
            .vault()?
            .data
            .responses
            .iter()
            .find(|r| r.request_id == id)
            .ok_or(Error::NotFound)?;
        storage::atomic_write(&path, &serde_json::to_vec_pretty(r)?)
    })
}
#[tauri::command]
fn create_backup(
    state: State<AppState>,
    path: PathBuf,
    password: String,
    confirmed: bool,
) -> Result<()> {
    extension(&path, "avvault")?;
    let password = Zeroizing::new(password);
    with(&state, |s| s.backup(&path, &password, confirmed))
}
#[tauri::command]
fn restore_backup(
    state: State<AppState>,
    path: PathBuf,
    password: String,
    dry_run: bool,
) -> Result<()> {
    extension(&path, "avvault")?;
    let password = Zeroizing::new(password);
    with(&state, |s| s.restore(&path, &password, dry_run))
}
#[tauri::command]
fn review_request(state: State<AppState>, id: Uuid) -> Result<Review> {
    with(&state, |s| s.review(id))
}
#[tauri::command]
fn approve_signing(
    state: State<AppState>,
    token: Uuid,
    confirmed: bool,
    password: String,
    key_passwords: HashMap<Uuid, String>,
    selected_accounts: Vec<Uuid>,
) -> Result<protocol::Response> {
    let password = Zeroizing::new(password);
    let passwords = key_passwords
        .into_iter()
        .map(|(id, value)| (id, Zeroizing::new(value)))
        .collect();
    with(&state, |s| {
        s.sign(token, confirmed, &password, &passwords, &selected_accounts)
    })
}
#[cfg(feature = "network")]
#[tauri::command]
fn prepare_transfer(
    state: State<AppState>,
    network: Network,
    utxos: Vec<Utxo>,
    recipients: Vec<Recipient>,
    change: String,
    fee_sats: u64,
) -> Result<Uuid> {
    with(&state, |s| {
        let vault = s.vault()?;
        if !vault.data.settings.mode.can_network() {
            return Err(Error::Mode);
        }
        let r = bitcoin::construct(
            network,
            utxos,
            recipients,
            &change,
            fee_sats,
            &vault.data.accounts,
        )?;
        s.import_request(&serde_json::to_vec(&r)?)
    })
}
#[cfg(feature = "network")]
#[tauri::command]
fn prepare_chain_transfer(
    state: State<AppState>,
    account_id: Uuid,
    operation: airvault_core::chains::transactions::Operation,
) -> Result<Uuid> {
    with(&state, |s| {
        let vault = s.vault()?;
        if !vault.data.settings.mode.can_network() {
            return Err(Error::Mode);
        }
        let account = vault
            .data
            .accounts
            .iter()
            .find(|a| a.id == account_id)
            .ok_or(Error::NotFound)?;
        let transaction = airvault_core::chains::transactions::Transaction {
            sender: account.address.clone(),
            public_key: account.public_key.clone(),
            operation,
        };
        let request = airvault_core::chains::transactions::request(account.network, transaction)?;
        s.import_request(&serde_json::to_vec(&request)?)
    })
}
#[cfg(feature = "network")]
fn public_context(state: &State<AppState>, account_id: Uuid) -> Result<(Settings, model::Account)> {
    with(state, |s| {
        let vault = s.vault()?;
        if !vault.data.settings.mode.can_network() {
            return Err(Error::Mode);
        }
        Ok((
            vault.data.settings.clone(),
            vault
                .data
                .accounts
                .iter()
                .find(|a| a.id == account_id)
                .ok_or(Error::NotFound)?
                .clone(),
        ))
    })
}
#[cfg(feature = "network")]
#[tauri::command]
async fn fetch_utxos(state: State<'_, AppState>, account_id: Uuid) -> Result<Vec<Utxo>> {
    let (settings, account) = public_context(&state, account_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        airvault_core::network::utxos(
            settings.mode,
            &settings.provider(account.network),
            &account.address,
            account.network,
        )
    })
    .await
    .map_err(|_| Error::Network)?
}
#[cfg(feature = "network")]
#[tauri::command]
async fn fetch_chain_state(
    state: State<'_, AppState>,
    account_id: Uuid,
) -> Result<serde_json::Value> {
    let (settings, account) = public_context(&state, account_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        airvault_core::providers::fetch_state(&settings, &account)
    })
    .await
    .map_err(|_| Error::Network)?
}
#[cfg(feature = "network")]
#[tauri::command]
async fn address_summary(
    state: State<'_, AppState>,
    account_id: Uuid,
) -> Result<serde_json::Value> {
    let (settings, account) = public_context(&state, account_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        airvault_core::providers::balance(&settings, &account)
    })
    .await
    .map_err(|_| Error::Network)?
}
#[cfg(feature = "network")]
#[tauri::command]
async fn broadcast_response(
    state: State<'_, AppState>,
    id: Uuid,
    password: String,
    confirmed: bool,
) -> Result<String> {
    let password = Zeroizing::new(password);
    let (settings, response, digest) = with(&state, |s| {
        if !confirmed {
            return Err(Error::Approval);
        }
        let vault = s.vault()?;
        vault.authenticate(&password)?;
        if !vault.data.settings.mode.can_network() {
            return Err(Error::Mode);
        }
        let request = vault
            .data
            .pending
            .iter()
            .find(|r| r.request_id == id)
            .ok_or(Error::NotFound)?;
        let response = vault
            .data
            .responses
            .iter()
            .find(|r| r.request_id == id)
            .ok_or(Error::NotFound)?;
        airvault_core::chains::transactions::verify(request, response, &vault.data.accounts)?;
        Ok((
            vault.data.settings.clone(),
            response.clone(),
            request.binding()?,
        ))
    })?;
    drop(password);
    let txid = tauri::async_runtime::spawn_blocking(move || {
        airvault_core::providers::broadcast(&settings, &response)
    })
    .await
    .map_err(|_| Error::Network)??;
    with(&state, |s| {
        s.record_broadcast(model::History {
            id: Uuid::new_v4(),
            request_id: id,
            action: "broadcast".into(),
            at: model::now(),
            digest,
            txid: Some(txid.clone()),
        });
        Ok(())
    })?;
    Ok(txid)
}
#[tauri::command]
async fn app_info() -> Result<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(|| {
        let binary = airvault_core::integrity::current_binary()?;
        Ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "buildHash": env!("AIRVAULT_BUILD_HASH"),
            "sourceHash": env!("AIRVAULT_SOURCE_HASH"),
            "builtAt": env!("AIRVAULT_BUILT_AT"),
            "target": env!("AIRVAULT_BUILD_TARGET"),
            "dirty": env!("AIRVAULT_DIRTY") == "true",
            "binaryPath": binary.path,
            "binarySha256": binary.sha256,
        }))
    })
    .await
    .map_err(|_| Error::Invalid("Could not read application identity"))?
}
#[tauri::command]
fn save_whitepaper(path: PathBuf) -> Result<()> {
    // A bundled public document only: this command never reads vault records.
    let document = include_bytes!("../../public/reference/bitcoin-whitepaper.pdf");
    storage::atomic_write(&path, document)
}
#[tauri::command]
fn capabilities() -> Vec<airvault_core::capabilities::Capability> {
    airvault_core::capabilities::registry()
}

fn main() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init()).setup(|app| {
        let mut directory = app.path().app_data_dir()?;
        let args: Vec<String> = std::env::args().collect();
        if let Some(i) = args.iter().position(|a| a == "--data-dir") {
            directory = PathBuf::from(args.get(i + 1).ok_or("missing --data-dir argument")?);
        }
        std::fs::create_dir_all(&directory)?;
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?; }
        let lock = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(directory.join("session.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock)?;
        let session = Arc::new(Mutex::new(Session::new(directory.join("vault.avvault"))));
        #[cfg(windows)] power::register(&session)?;
        let timer = Arc::clone(&session);
        std::thread::spawn(move || loop { std::thread::sleep(Duration::from_secs(2)); if let Ok(mut s) = timer.lock() { #[cfg(windows)] if power::take_lock_request() { s.lock(); } s.tick(); } });
        app.manage(AppState { session, _lock: lock });
        eprintln!("{}", serde_json::json!({"level":"info","loggerKey":"AIRVAULT_STARTUP","version":env!("CARGO_PKG_VERSION"),"buildHash":env!("AIRVAULT_BUILD_HASH"),"sourceHash":env!("AIRVAULT_SOURCE_HASH"),"target":env!("AIRVAULT_BUILD_TARGET")}));
        Ok(())
    }).on_window_event(|window, event| {
        if let tauri::WindowEvent::Focused(false) = event {
            if let Some(state) = window.try_state::<AppState>() { if let Ok(mut s) = state.session.lock() { s.focus_lost(); } }
        }
    });
    #[cfg(feature = "network")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        snapshot,
        app_info,
        inspect_file,
        inspect_text,
        inspect_exchange,
        save_whitepaper,
        create_vault,
        unlock_vault,
        lock_vault,
        activity,
        cancel_ceremony,
        generate_phrase,
        confirm_phrase,
        import_phrase,
        import_wif,
        add_identity,
        derive_account,
        multisig_cosigner,
        preview_multisig,
        register_multisig,
        read_multisig,
        export_multisig,
        import_psbt,
        export_psbt,
        export_transaction,
        approve_multisig,
        add_account_token,
        delete_account,
        verify_recovery,
        change_password,
        save_settings,
        import_file,
        export_public,
        export_request,
        export_response,
        create_backup,
        restore_backup,
        review_request,
        approve_signing,
        prepare_transfer,
        prepare_chain_transfer,
        fetch_utxos,
        fetch_chain_state,
        address_summary,
        broadcast_response,
        capabilities
    ]);
    #[cfg(not(feature = "network"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        snapshot,
        app_info,
        inspect_file,
        inspect_text,
        inspect_exchange,
        save_whitepaper,
        create_vault,
        unlock_vault,
        lock_vault,
        activity,
        cancel_ceremony,
        generate_phrase,
        confirm_phrase,
        import_phrase,
        import_wif,
        add_identity,
        derive_account,
        multisig_cosigner,
        preview_multisig,
        register_multisig,
        read_multisig,
        export_multisig,
        import_psbt,
        export_psbt,
        export_transaction,
        approve_multisig,
        add_account_token,
        delete_account,
        verify_recovery,
        change_password,
        save_settings,
        import_file,
        export_public,
        export_request,
        export_response,
        create_backup,
        restore_backup,
        review_request,
        approve_signing,
        capabilities
    ]);
    builder
        .run(tauri::generate_context!())
        .expect("AirVault could not start");
}

#[tauri::command]
fn multisig_cosigner(
    state: State<AppState>,
    identity_id: Uuid,
    network: Network,
    account: u32,
    key_password: Option<String>,
) -> Result<String> {
    let password = key_password.map(Zeroizing::new);
    with(&state, |s| {
        Ok(s.vault()?
            .multisig_cosigner(
                identity_id,
                network,
                account,
                password.as_ref().map(|p| p.as_str()),
            )?
            .text())
    })
}
#[tauri::command]
fn preview_multisig(
    label: String,
    network: Network,
    threshold: u8,
    cosigners: String,
    branch: u32,
    index: u32,
) -> Result<airvault_core::multisig::Backup> {
    if cosigners.len() > 4096 {
        return Err(Error::Invalid("cosigner list size"));
    }
    let policy = airvault_core::multisig::Policy {
        threshold,
        branch,
        index,
        cosigners: cosigners
            .lines()
            .filter(|s| !s.trim().is_empty())
            .map(airvault_core::multisig::Cosigner::parse)
            .collect::<Result<Vec<_>>>()?,
    };
    airvault_core::multisig::Backup::new(&label, network, policy)
}
#[tauri::command]
fn register_multisig(
    state: State<AppState>,
    backup: airvault_core::multisig::Backup,
    identity_id: Option<Uuid>,
    key_password: Option<String>,
    confirmed: bool,
) -> Result<Uuid> {
    if !confirmed {
        return Err(Error::Approval);
    }
    let backup = airvault_core::multisig::Backup::parse(&serde_json::to_vec(&backup)?)?;
    let password = key_password.map(Zeroizing::new);
    with(&state, |s| {
        s.mutate(|v| {
            v.register_multisig(
                &backup.label,
                backup.network,
                backup.policy,
                identity_id,
                password.as_ref().map(|p| p.as_str()),
            )
        })
    })
}
#[tauri::command]
fn read_multisig(path: PathBuf) -> Result<airvault_core::multisig::Backup> {
    extension(&path, "avmulti")?;
    airvault_core::multisig::Backup::parse(&storage::read_bounded(&path, 32768)?)
}
#[tauri::command]
fn export_multisig(state: State<AppState>, id: Uuid, path: PathBuf) -> Result<()> {
    extension(&path, "avmulti")?;
    with(&state, |s| {
        let account = s
            .vault()?
            .data
            .accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or(Error::NotFound)?;
        let backup = airvault_core::multisig::Backup::new(
            &account.label,
            account.network,
            account.multisig.clone().ok_or(Error::NotFound)?,
        )?;
        storage::atomic_write(&path, &serde_json::to_vec_pretty(&backup)?)
    })
}
#[tauri::command]
fn import_psbt(state: State<AppState>, path: PathBuf, network: Network) -> Result<Uuid> {
    extension(&path, "psbt")?;
    with(&state, |s| {
        s.import_psbt(
            &storage::read_bounded(&path, protocol::MAX_EXCHANGE_BYTES as u64)?,
            network,
        )
    })
}
#[tauri::command]
fn export_psbt(state: State<AppState>, path: PathBuf, id: Uuid) -> Result<()> {
    extension(&path, "psbt")?;
    with(&state, |s| storage::atomic_write(&path, &s.psbt_bytes(id)?))
}
#[tauri::command]
fn export_transaction(state: State<AppState>, path: PathBuf, id: Uuid) -> Result<()> {
    extension(&path, "txn")?;
    with(&state, |s| {
        // Sparrow's .txn convention is a UTF-8 hex representation of the complete raw transaction.
        storage::atomic_write(&path, hex_transaction(&s.transaction_bytes(id)?).as_bytes())
    })
}
fn hex_transaction(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
#[tauri::command]
fn approve_multisig(
    state: State<AppState>,
    token: Uuid,
    confirmed: bool,
    password: String,
    key_passwords: HashMap<Uuid, String>,
    selected_accounts: Vec<Uuid>,
) -> Result<Uuid> {
    let password = Zeroizing::new(password);
    let keys = key_passwords
        .into_iter()
        .map(|(id, value)| (id, Zeroizing::new(value)))
        .collect();
    with(&state, |s| {
        s.sign_multisig(token, confirmed, &password, &keys, &selected_accounts)
    })
}
