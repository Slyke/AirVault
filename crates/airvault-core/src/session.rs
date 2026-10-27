use crate::{
    bitcoin::Inspection,
    chains::transactions as bitcoin,
    entropy,
    model::{self, Account, History, Identity, Mode, Root, Settings, VaultData},
    protocol::{PublicBundle, Request, Response},
    storage::{self, OpenVault},
    Error, Result,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub initialized: bool,
    pub unlocked: bool,
    pub network_build: bool,
    pub settings: Option<Settings>,
    pub roots: Vec<Root>,
    pub identities: Vec<Identity>,
    pub accounts: Vec<Account>,
    pub history: Vec<History>,
    pub pending: Vec<Request>,
    pub responses: Vec<Response>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ceremony {
    pub token: Uuid,
    pub phrase: String,
}
struct PendingCeremony {
    token: Uuid,
    phrase: Zeroizing<String>,
    at: Instant,
}
struct Approval {
    token: Uuid,
    request_id: Uuid,
    digest: String,
    at: Instant,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub token: Uuid,
    pub inspection: Inspection,
}
pub struct Session {
    broadcast_receipts: Vec<History>,
    path: PathBuf,
    vault: Option<OpenVault>,
    ceremony: Option<PendingCeremony>,
    approval: Option<Approval>,
    last_activity: Instant,
    last_tick: Instant,
}
impl Session {
    pub fn new(path: PathBuf) -> Self {
        Self {
            broadcast_receipts: vec![],
            path,
            vault: None,
            ceremony: None,
            approval: None,
            last_activity: Instant::now(),
            last_tick: Instant::now(),
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        let data = self.vault.as_ref().map(|v| &v.data);
        Snapshot {
            initialized: self.path.exists(),
            unlocked: data.is_some(),
            network_build: cfg!(feature = "network"),
            settings: data.map(|d| d.settings.clone()),
            roots: data.map(|d| d.roots.clone()).unwrap_or_default(),
            identities: data.map(|d| d.identities.clone()).unwrap_or_default(),
            accounts: data.map(|d| d.accounts.clone()).unwrap_or_default(),
            history: data.map(|d| d.history.clone()).unwrap_or_default(),
            pending: data.map(|d| d.pending.clone()).unwrap_or_default(),
            responses: data.map(|d| d.responses.clone()).unwrap_or_default(),
        }
    }
    pub fn create(&mut self, password: &str, mode: Mode) -> Result<()> {
        if self.path.exists() {
            return Err(Error::Invalid(
                "a vault already exists; restore only on a fresh profile",
            ));
        }
        if !cfg!(feature = "network") && mode != Mode::Offline {
            return Err(Error::Mode);
        }
        let mut data = VaultData::default();
        data.settings.mode = mode;
        let mut vault = OpenVault::create(password, data)?;
        vault.save(&self.path)?;
        self.vault = Some(vault);
        self.flush_receipts();
        self.touch();
        Ok(())
    }
    pub fn unlock(&mut self, password: &str) -> Result<()> {
        self.lock();
        let vault = OpenVault::unlock(
            &storage::read_bounded(&self.path, storage::MAX_VAULT_BYTES)?,
            password,
        )?;
        if !cfg!(feature = "network") && vault.data.settings.mode != Mode::Offline {
            return Err(Error::Mode);
        }
        if vault.data.settings.mode == Mode::Proxy && !vault.data.records.is_empty() {
            return Err(Error::Mode);
        }
        self.vault = Some(vault);
        self.flush_receipts();
        self.touch();
        Ok(())
    }
    /// Public receipts survive an automatic lock; their encrypted history is saved on unlock.
    pub fn record_broadcast(&mut self, receipt: History) {
        if !self
            .broadcast_receipts
            .iter()
            .any(|r| r.request_id == receipt.request_id && r.txid == receipt.txid)
        {
            self.broadcast_receipts.push(receipt);
        }
        self.flush_receipts();
    }
    fn flush_receipts(&mut self) {
        if self.vault.is_none() || self.broadcast_receipts.is_empty() {
            return;
        }
        let receipts = self.broadcast_receipts.clone();
        if self
            .mutate(|v| {
                for receipt in &receipts {
                    let matching = v.data.pending.iter().any(|r| {
                        r.request_id == receipt.request_id
                            && r.binding().ok().as_deref() == Some(receipt.digest.as_str())
                    });
                    if matching
                        && !v.data.history.iter().any(|h| {
                            h.action == "broadcast"
                                && h.request_id == receipt.request_id
                                && h.txid == receipt.txid
                        })
                    {
                        v.data.history.push(receipt.clone());
                    }
                }
                Ok(())
            })
            .is_ok()
        {
            self.broadcast_receipts.clear();
        }
    }
    pub fn cancel_ceremony(&mut self) {
        self.ceremony = None;
    }
    pub fn lock(&mut self) {
        self.ceremony = None;
        self.approval = None;
        self.vault = None;
    }
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }
    pub fn tick(&mut self) {
        let missed = self.last_tick.elapsed() > Duration::from_secs(15);
        self.last_tick = Instant::now();
        if self.vault.as_ref().is_some_and(|v| {
            v.data.settings.timeout_seconds != -1
                && self.last_activity.elapsed().as_secs()
                    >= v.data.settings.timeout_seconds.max(0) as u64
        }) || missed
        {
            self.lock();
        }
        if self
            .ceremony
            .as_ref()
            .is_some_and(|c| c.at.elapsed() > Duration::from_secs(300))
        {
            self.ceremony = None;
        }
    }
    pub fn focus_lost(&mut self) {
        if self
            .vault
            .as_ref()
            .is_some_and(|v| v.data.settings.lock_on_blur)
        {
            self.lock();
        }
    }
    pub fn vault(&self) -> Result<&OpenVault> {
        self.vault.as_ref().ok_or(Error::Locked)
    }
    /// Roll back metadata and wrapping changes when an operation or durable save fails.
    pub fn mutate<T>(&mut self, operation: impl FnOnce(&mut OpenVault) -> Result<T>) -> Result<T> {
        let vault = self.vault.as_mut().ok_or(Error::Locked)?;
        let previous_data = vault.data.clone();
        let previous_file = vault.file.clone();
        let outcome = operation(vault).and_then(|value| {
            vault.save(&self.path)?;
            Ok(value)
        });
        if outcome.is_err() {
            vault.data = previous_data;
            vault.file = previous_file;
        }
        self.touch();
        outcome
    }
    pub fn generate(&mut self, words: u8, prefix: &str, bits: Option<&str>) -> Result<Ceremony> {
        if !self.vault()?.data.settings.mode.can_sign() {
            return Err(Error::Mode);
        }
        let phrase = entropy::generate(words, prefix, bits)?;
        let token = Uuid::new_v4();
        let ceremony = Ceremony {
            token,
            phrase: phrase.to_string(),
        };
        self.ceremony = Some(PendingCeremony {
            token,
            phrase,
            at: Instant::now(),
        });
        self.touch();
        Ok(ceremony)
    }
    pub fn confirm_phrase(
        &mut self,
        token: Uuid,
        confirmation: &str,
        label: &str,
        password: Option<&str>,
    ) -> Result<Uuid> {
        use subtle::ConstantTimeEq;
        self.tick();
        let pending = self.ceremony.as_ref().ok_or(Error::Approval)?;
        if pending.token != token || pending.at.elapsed() > Duration::from_secs(300) {
            return Err(Error::Approval);
        }
        let confirmation = entropy::validate(confirmation)?;
        if !bool::from(confirmation.as_bytes().ct_eq(pending.phrase.as_bytes())) {
            return Err(Error::Invalid(
                "Recovery confirmation did not match. Check your words and try again",
            ));
        }
        let id = self.mutate(|v| v.import_phrase(label, &confirmation, password))?;
        self.ceremony = None;
        Ok(id)
    }
    pub fn delete_account(&mut self, account_id: Uuid, confirmed: bool) -> Result<()> {
        if !confirmed {
            return Err(Error::Approval);
        }
        self.mutate(|v| {
            let index = v
                .data
                .accounts
                .iter()
                .position(|a| a.id == account_id)
                .ok_or(Error::NotFound)?;
            v.data.accounts.remove(index);
            Ok(())
        })?;
        self.approval = None;
        Ok(())
    }

    pub fn import_public(&mut self, bytes: &[u8]) -> Result<()> {
        let bundle = PublicBundle::parse(bytes)?;
        self.mutate(|v| {
            for mut account in bundle.accounts {
                if v.data
                    .accounts
                    .iter()
                    .any(|a| a.network == account.network && a.address == account.address)
                {
                    continue;
                }
                account.id = Uuid::new_v4();
                v.data.accounts.push(account);
            }
            Ok(())
        })
    }
    pub fn public_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(&PublicBundle::new(
            self.vault()?.public_accounts(),
        )?)?)
    }
    pub fn import_request(&mut self, bytes: &[u8]) -> Result<Uuid> {
        let request = Request::parse(bytes)?;
        let id = request.request_id;
        bitcoin::inspect(&request, &self.vault()?.data.accounts)?;
        self.mutate(|v| {
            if v.data.pending.iter().any(|r| r.request_id == id)
                || v.data
                    .history
                    .iter()
                    .any(|h| h.request_id == id && h.action == "signed")
            {
                return Err(Error::Duplicate);
            }
            if v.data.pending.len() >= 500 {
                return Err(Error::Invalid("pending request limit"));
            }
            v.data.pending.push(request);
            Ok(id)
        })
    }
    pub fn review(&mut self, id: Uuid) -> Result<Review> {
        let vault = self.vault()?;
        let request = vault
            .data
            .pending
            .iter()
            .find(|r| r.request_id == id)
            .ok_or(Error::NotFound)?;
        let mut inspection =
            if let Some(response) = vault.data.responses.iter().find(|r| r.request_id == id) {
                bitcoin::verify(request, response, &vault.data.accounts)?
            } else {
                bitcoin::inspect(request, &vault.data.accounts)?
            };
        if let Some(progress) = vault.data.psbts.iter().find(|p| p.request_id == id) {
            let current = crate::multisig_psbt::decode(&progress.payload.decode()?)?;
            inspection = crate::bitcoin::inspect(
                &crate::multisig_psbt::request_with(request, &current),
                &vault.data.accounts,
            )?;
            inspection.digest = request.binding()?;
        }
        let token = Uuid::new_v4();
        self.approval = Some(Approval {
            token,
            request_id: id,
            digest: inspection.digest.clone(),
            at: Instant::now(),
        });
        self.touch();
        Ok(Review { token, inspection })
    }
    pub fn sign(
        &mut self,
        token: Uuid,
        confirmed: bool,
        master_password: &str,
        key_passwords: &HashMap<Uuid, Zeroizing<String>>,
        selected_accounts: &[Uuid],
    ) -> Result<Response> {
        let approval = self.approval.take().ok_or(Error::Approval)?;
        if !confirmed || token != approval.token || approval.at.elapsed() > Duration::from_secs(120)
        {
            return Err(Error::Approval);
        }
        self.mutate(|vault| {
            if !vault.data.settings.mode.can_sign() {
                return Err(Error::Mode);
            }
            vault.authenticate(master_password)?;
            let request = vault
                .data
                .pending
                .iter()
                .find(|r| r.request_id == approval.request_id)
                .ok_or(Error::NotFound)?
                .clone();
            if request.binding()? != approval.digest {
                return Err(Error::Approval);
            }
            if vault
                .data
                .history
                .iter()
                .any(|h| h.request_id == request.request_id && h.action == "signed")
            {
                return Err(Error::Duplicate);
            }
            let inspection = bitcoin::inspect(&request, &vault.data.accounts)?;
            let mut keys = vec![];
            for input in &inspection.inputs {
                let matches: Vec<_> = vault
                    .data
                    .accounts
                    .iter()
                    .filter(|a| {
                        input.account_ids.contains(&a.id)
                            && !a.watch_only
                            && a.identity_id.is_some()
                    })
                    .collect();
                let account = if matches.len() == 1 {
                    matches[0]
                } else {
                    let selected: Vec<_> = matches
                        .iter()
                        .filter(|a| selected_accounts.contains(&a.id))
                        .collect();
                    if selected.len() != 1 {
                        return Err(Error::Invalid(
                            "select exactly one matching local account for each ambiguous input",
                        ));
                    }
                    *selected[0]
                };
                let identity_id = account.identity_id.ok_or(Error::NotFound)?;
                let identity = vault
                    .data
                    .identities
                    .iter()
                    .find(|i| i.id == identity_id)
                    .ok_or(Error::NotFound)?;
                let password = key_passwords.get(&identity.root_id).map(|p| p.as_str());
                let (key, _) =
                    vault.derive_material(identity_id, account.network, &account.path, password)?;
                let (address, public) =
                    crate::chains::keys::public_account(&key, account.network, &account.path)?;
                if address != input.address || public != account.public_key {
                    return Err(Error::Verification);
                }
                keys.push(key);
            }
            let response = bitcoin::sign(&request, &vault.data.accounts, &keys)?;
            vault.data.responses.push(response.clone());
            vault.data.history.push(History {
                id: Uuid::new_v4(),
                request_id: request.request_id,
                action: "signed".into(),
                at: model::now(),
                digest: approval.digest,
                txid: Some(response.txid.clone()),
            });
            Ok(response)
        })
    }
    pub fn import_response(&mut self, bytes: &[u8]) -> Result<Inspection> {
        let response = Response::parse(bytes)?;
        self.mutate(|v| {
            let request = v
                .data
                .pending
                .iter()
                .find(|r| r.request_id == response.request_id)
                .ok_or(Error::NotFound)?;
            let inspection = bitcoin::verify(request, &response, &v.data.accounts)?;
            if v.data
                .responses
                .iter()
                .any(|r| r.request_id == response.request_id)
            {
                return Err(Error::Duplicate);
            }
            v.data.history.push(History {
                id: Uuid::new_v4(),
                request_id: request.request_id,
                action: "verified".into(),
                at: model::now(),
                digest: response.request_digest.clone(),
                txid: Some(response.txid.clone()),
            });
            v.data.responses.push(response);
            Ok(inspection)
        })
    }
    pub fn settings(&mut self, settings: Settings, password: &str, confirmed: bool) -> Result<()> {
        if !confirmed {
            return Err(Error::Approval);
        }
        if settings.timeout_seconds != -1 && !(30..=3600).contains(&settings.timeout_seconds) {
            return Err(Error::Invalid(
                "Auto-lock must be -1 (never) or 30 to 3600 seconds",
            ));
        }
        if settings.rpc_url.len() > 2048
            || settings.providers.len() > 30
            || settings
                .providers
                .iter()
                .any(|(k, v)| k.len() > 30 || v.len() > 2048)
        {
            return Err(Error::Invalid("settings bounds"));
        }
        if !cfg!(feature = "network") && settings.mode != Mode::Offline {
            return Err(Error::Mode);
        }
        self.approval = None;
        self.mutate(|v| {
            v.authenticate(password)?;
            if settings.mode == Mode::Proxy && !v.data.records.is_empty() {
                return Err(Error::Invalid(
                    "proxy mode requires a vault containing only watch-only accounts",
                ));
            }
            v.data.settings = settings;
            Ok(())
        })
    }
    pub fn backup(&mut self, destination: &Path, password: &str, confirmed: bool) -> Result<()> {
        if !confirmed {
            return Err(Error::Approval);
        }
        if destination == self.path {
            return Err(Error::Invalid("backup destination is the active vault"));
        }
        let vault = self.vault.as_mut().ok_or(Error::Locked)?;
        vault.authenticate(password)?;
        storage::atomic_write(destination, &vault.encrypted_bytes()?)
    }
    pub fn restore(&mut self, source: &Path, password: &str, dry_run: bool) -> Result<()> {
        let bytes = storage::read_bounded(source, storage::MAX_VAULT_BYTES)?;
        let vault = OpenVault::unlock(&bytes, password)?;
        if !dry_run {
            if self.path.exists() {
                return Err(Error::Invalid(
                    "restore requires a fresh application data directory",
                ));
            }
            if !cfg!(feature = "network") && vault.data.settings.mode != Mode::Offline {
                return Err(Error::Mode);
            }
            storage::atomic_write(&self.path, &bytes)?;
            self.lock();
        }
        Ok(())
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn expiry_cancellation_and_lock_invalidate_creation_ceremonies() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::new(dir.path().join("vault.avvault"));
        session
            .create("public-test-password", Mode::Offline)
            .unwrap();
        let ceremony = session.generate(12, "", Some(&"0".repeat(128))).unwrap();
        session.cancel_ceremony();
        assert!(session
            .confirm_phrase(ceremony.token, &ceremony.phrase, "Cancelled", None)
            .is_err());
        let ceremony = session.generate(12, "", Some(&"0".repeat(128))).unwrap();
        session.ceremony.as_mut().unwrap().at = Instant::now() - Duration::from_secs(301);
        session.tick();
        assert!(session
            .confirm_phrase(ceremony.token, &ceremony.phrase, "Expired", None)
            .is_err());
        session.last_activity = Instant::now() - Duration::from_secs(301);
        session.tick();
        assert!(!session.snapshot().unlocked);
        assert!(session.snapshot().roots.is_empty());
    }
    #[test]
    fn never_inactivity_lock_persists_but_explicit_and_lifecycle_locks_remain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.avvault");
        let mut session = Session::new(path.clone());
        session
            .create("public-test-password", Mode::Offline)
            .unwrap();
        let original = session.vault().unwrap().data.settings.clone();
        for invalid in [-2, 0, 29, 3601] {
            let mut settings = original.clone();
            settings.timeout_seconds = invalid;
            assert!(session
                .settings(settings, "public-test-password", true)
                .is_err());
        }
        let mut settings = original;
        settings.timeout_seconds = -1;
        session
            .settings(settings, "public-test-password", true)
            .unwrap();
        session.last_activity = Instant::now() - Duration::from_secs(86_400);
        session.tick();
        assert!(session.snapshot().unlocked);
        session.lock();
        assert!(!session.snapshot().unlocked);
        session.unlock("public-test-password").unwrap();
        assert_eq!(session.vault().unwrap().data.settings.timeout_seconds, -1);
        session.last_tick = Instant::now() - Duration::from_secs(16);
        session.tick();
        assert!(!session.snapshot().unlocked);
        session.unlock("public-test-password").unwrap();
        session
            .mutate(|v| {
                v.data.settings.lock_on_blur = true;
                Ok(())
            })
            .unwrap();
        session.focus_lost();
        assert!(!session.snapshot().unlocked);
        drop(session);
        assert!(!Session::new(path).snapshot().unlocked);
    }
    #[test]
    fn phrase_confirmation_retries_without_losing_the_ceremony() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::new(dir.path().join("vault.avvault"));
        session
            .create("public-test-password", Mode::Offline)
            .unwrap();
        let ceremony = session.generate(12, "", Some(&"0".repeat(128))).unwrap();
        assert!(session
            .confirm_phrase(ceremony.token, "typo", "Test", None)
            .is_err());
        let different = entropy::generate(12, "", Some(&"1".repeat(128))).unwrap();
        assert!(session
            .confirm_phrase(ceremony.token, &different, "Test", None)
            .is_err());
        assert!(session.ceremony.is_some());
        session
            .confirm_phrase(ceremony.token, &ceremony.phrase, "Test", None)
            .unwrap();
        assert!(session.ceremony.is_none());
        assert!(session
            .confirm_phrase(ceremony.token, &ceremony.phrase, "Test", None)
            .is_err());
        assert_eq!(session.snapshot().roots.len(), 1);
    }
    #[test]
    fn account_deletion_preserves_recovery_and_invalidates_approval() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::new(dir.path().join("vault.avvault"));
        session
            .create("public-test-password", Mode::Offline)
            .unwrap();
        let phrase = entropy::generate(12, "", Some(&"0".repeat(128))).unwrap();
        let identity = session
            .mutate(|v| v.import_phrase("Test", &phrase, None))
            .unwrap();
        let path = "m/84'/1'/0'/0/0";
        let id = session
            .mutate(|v| {
                v.derive_account(identity, "Test", crate::model::Network::Testnet, path, None)
            })
            .unwrap();
        let address = session.snapshot().accounts[0].address.clone();
        session.approval = Some(Approval {
            token: Uuid::new_v4(),
            request_id: Uuid::new_v4(),
            digest: String::new(),
            at: Instant::now(),
        });
        assert!(session.delete_account(id, false).is_err());
        assert_eq!(session.snapshot().accounts.len(), 1);
        session.delete_account(id, true).unwrap();
        assert!(session.approval.is_none());
        session.lock();
        session.unlock("public-test-password").unwrap();
        assert!(session.snapshot().accounts.is_empty());
        assert_eq!(session.snapshot().roots.len(), 1);
        session
            .mutate(|v| {
                v.derive_account(
                    identity,
                    "Restored",
                    crate::model::Network::Testnet,
                    path,
                    None,
                )
            })
            .unwrap();
        assert_eq!(session.snapshot().accounts[0].address, address);
        session.lock();
        assert!(session.delete_account(id, true).is_err());
    }
    #[test]
    fn expired_approval_fails_before_key_or_request_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::new(dir.path().join("vault.avvault"));
        session
            .create("public-test-password", Mode::Offline)
            .unwrap();
        let token = Uuid::new_v4();
        session.approval = Some(Approval {
            token,
            request_id: Uuid::new_v4(),
            digest: String::new(),
            at: Instant::now() - Duration::from_secs(121),
        });
        assert!(matches!(
            session.sign(token, true, "public-test-password", &HashMap::new(), &[]),
            Err(Error::Approval)
        ));
        assert!(session.approval.is_none());
        session.last_tick = Instant::now() - Duration::from_secs(16);
        session.tick();
        assert!(!session.snapshot().unlocked);
    }
}

impl Session {
    pub fn psbt_bytes(&self, id: Uuid) -> Result<Vec<u8>> {
        let vault = self.vault()?;
        let request = vault
            .data
            .pending
            .iter()
            .find(|r| r.request_id == id)
            .ok_or(Error::NotFound)?;
        if let Some(progress) = vault.data.psbts.iter().find(|p| p.request_id == id) {
            return progress.payload.decode();
        }
        if let Some(response) = vault.data.responses.iter().find(|r| r.request_id == id) {
            return crate::bitcoin::signed_psbt(request, response, &vault.data.accounts);
        }
        request.validate()
    }
    /// Standard serialized Bitcoin transaction; exported only after complete independent verification.
    pub fn transaction_bytes(&self, id: Uuid) -> Result<Vec<u8>> {
        let vault = self.vault()?;
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
            .ok_or(Error::Invalid("Transaction is not fully signed"))?;
        crate::bitcoin::verify(request, response, &vault.data.accounts)?;
        response.payload.decode()
    }
    pub fn import_psbt(&mut self, bytes: &[u8], network: model::Network) -> Result<Uuid> {
        use crate::multisig_psbt as multi;
        let incoming = multi::decode(bytes)?;
        let probe = Request::bitcoin(network, &incoming.serialize(), "Bitcoin transfer".into());
        if !multi::is_multisig(&probe)? {
            // The ordinary adapter accepts only unsigned native SegWit P2WPKH with full previous transactions.
            crate::bitcoin::inspect(&probe, &self.vault()?.data.accounts)?;
            self.approval = None;
            return self.mutate(|v| {
                if let Some(existing) = v.data.pending.iter().find(|r| {
                    r.network == network
                        && crate::bitcoin::parse(r)
                            .is_ok_and(|p| p.unsigned_tx == incoming.unsigned_tx)
                }) {
                    return Ok(existing.request_id);
                }
                if v.data.pending.len() >= 500 {
                    return Err(Error::Invalid("pending request limit"));
                }
                let id = probe.request_id;
                v.data.pending.push(probe);
                Ok(id)
            });
        }
        crate::bitcoin::inspect(&probe, &self.vault()?.data.accounts)?;
        self.approval = None;
        self.mutate(|v| {
            let existing = v
                .data
                .pending
                .iter()
                .find(|r| {
                    r.network == network
                        && crate::bitcoin::parse(r)
                            .is_ok_and(|p| p.unsigned_tx == incoming.unsigned_tx)
                })
                .cloned();
            let request = if let Some(request) = existing {
                request
            } else {
                if v.data.pending.len() >= 500 {
                    return Err(Error::Invalid("pending request limit"));
                }
                let mut unsigned = incoming.clone();
                for input in &mut unsigned.inputs {
                    input.partial_sigs.clear();
                }
                let request = multi::request_with(&probe, &unsigned);
                v.data.pending.push(request.clone());
                request
            };
            let current = if let Some(progress) = v
                .data
                .psbts
                .iter()
                .find(|p| p.request_id == request.request_id)
            {
                multi::decode(&progress.payload.decode()?)?
            } else {
                crate::bitcoin::parse(&request)?
            };
            let merged = multi::merge(&request, &current, &incoming, &v.data.accounts)?;
            save_psbt_progress(v, &request, &merged)?;
            Ok(request.request_id)
        })
    }
    pub fn sign_multisig(
        &mut self,
        token: Uuid,
        confirmed: bool,
        master_password: &str,
        key_passwords: &HashMap<Uuid, Zeroizing<String>>,
        selected_accounts: &[Uuid],
    ) -> Result<Uuid> {
        use crate::multisig_psbt as multi;
        let approval = self.approval.take().ok_or(Error::Approval)?;
        if !confirmed || token != approval.token || approval.at.elapsed() > Duration::from_secs(120)
        {
            return Err(Error::Approval);
        }
        self.mutate(|v| {
            if !v.data.settings.mode.can_sign() {
                return Err(Error::Mode);
            }
            v.authenticate(master_password)?;
            let request = v
                .data
                .pending
                .iter()
                .find(|r| r.request_id == approval.request_id)
                .ok_or(Error::NotFound)?
                .clone();
            if request.binding()? != approval.digest || !multi::is_multisig(&request)? {
                return Err(Error::Approval);
            }
            if v.data
                .responses
                .iter()
                .any(|r| r.request_id == request.request_id)
            {
                return Err(Error::Duplicate);
            }
            let current = if let Some(progress) = v
                .data
                .psbts
                .iter()
                .find(|p| p.request_id == request.request_id)
            {
                multi::decode(&progress.payload.decode()?)?
            } else {
                crate::bitcoin::parse(&request)?
            };
            let inspection = crate::bitcoin::inspect(
                &multi::request_with(&request, &current),
                &v.data.accounts,
            )?;
            let mut keys = vec![];
            for (index, input) in inspection.inputs.iter().enumerate() {
                let matches: Vec<_> = v
                    .data
                    .accounts
                    .iter()
                    .filter(|a| {
                        input.account_ids.contains(&a.id)
                            && a.identity_id.is_some()
                            && !a.watch_only
                            && a.multisig.is_some()
                    })
                    .collect();
                // Missing local keys are expected in a multisig round.
                if matches.is_empty() {
                    continue;
                }
                let account = if matches.len() == 1 {
                    matches[0]
                } else {
                    let selected: Vec<_> = matches
                        .iter()
                        .filter(|a| selected_accounts.contains(&a.id))
                        .collect();
                    if selected.len() != 1 {
                        return Err(Error::Invalid("select one local cosigner for each input"));
                    }
                    *selected[0]
                };
                let identity_id = account.identity_id.ok_or(Error::Verification)?;
                let identity = v
                    .data
                    .identities
                    .iter()
                    .find(|i| i.id == identity_id)
                    .ok_or(Error::NotFound)?;
                let password = key_passwords.get(&identity.root_id).map(|p| p.as_str());
                let policy = account.multisig.as_ref().ok_or(Error::Verification)?;
                policy.validate_account(account)?;
                let (secret, fingerprint) =
                    v.derive_secret(identity_id, account.network, &account.path, password)?;
                if fingerprint != account.fingerprint {
                    return Err(Error::Verification);
                }
                let mut key = ::bitcoin::secp256k1::SecretKey::from_slice(&*secret)
                    .map_err(|_| Error::Verification)?;
                let public =
                    ::bitcoin::PublicKey::new(::bitcoin::secp256k1::PublicKey::from_secret_key(
                        &::bitcoin::secp256k1::Secp256k1::new(),
                        &key,
                    ));
                key.non_secure_erase();
                if public.to_string() != account.public_key
                    || !policy
                        .derived(request.network)?
                        .iter()
                        .any(|k| k.0 == public)
                {
                    return Err(Error::Verification);
                }
                if !current.inputs[index].partial_sigs.contains_key(&public) {
                    keys.push((index, secret));
                }
            }
            let signed = multi::sign(&request, &current, &v.data.accounts, &keys)?;
            save_psbt_progress(v, &request, &signed)?;
            v.data.history.push(History {
                id: Uuid::new_v4(),
                request_id: request.request_id,
                action: "cosigned".into(),
                at: model::now(),
                digest: approval.digest,
                txid: None,
            });
            Ok(request.request_id)
        })
    }
}
fn save_psbt_progress(
    v: &mut OpenVault,
    request: &Request,
    psbt: &::bitcoin::psbt::Psbt,
) -> Result<()> {
    let response = crate::multisig_psbt::finalize(request, psbt, &v.data.accounts)?;
    v.data.psbts.retain(|p| p.request_id != request.request_id);
    v.data.psbts.push(crate::multisig_psbt::Progress {
        request_id: request.request_id,
        payload: crate::protocol::Payload::new(&psbt.serialize()),
    });
    if let Some(response) = response {
        if !v
            .data
            .responses
            .iter()
            .any(|r| r.request_id == request.request_id)
        {
            v.data.history.push(History {
                id: Uuid::new_v4(),
                request_id: request.request_id,
                action: "finalized".into(),
                at: model::now(),
                digest: request.binding()?,
                txid: Some(response.txid.clone()),
            });
            v.data.responses.push(response);
        }
    }
    Ok(())
}
