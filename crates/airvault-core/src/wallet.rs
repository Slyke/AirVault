use crate::{
    entropy,
    model::{self, Account, Identity, Network, Root},
    storage::OpenVault,
    Error, Result,
};
use ::bitcoin::{
    bip32::{DerivationPath, Xpriv},
    secp256k1::{Secp256k1, SecretKey},
    Address, CompressedPublicKey, PrivateKey, PublicKey,
};
use bip39::Mnemonic;
use std::str::FromStr;
use uuid::Uuid;
use zeroize::Zeroizing;

impl OpenVault {
    pub fn import_phrase(
        &mut self,
        label: &str,
        phrase: &str,
        password: Option<&str>,
    ) -> Result<Uuid> {
        if !self.data.settings.mode.can_sign() {
            return Err(Error::Mode);
        }
        let label = model::label(label)?;
        let phrase = entropy::validate(phrase)?;
        let id = Uuid::new_v4();
        let record = self.encrypt_record(id, phrase.as_bytes(), password)?;
        self.data.records.push(record);
        self.data.roots.push(Root {
            id,
            label,
            description: String::new(),
            created_at: model::now(),
            kind: "bip39".into(),
            custom_password: password.is_some(),
            reauthenticate: true,
        });
        let identity = Uuid::new_v4();
        self.data.identities.push(Identity {
            id: identity,
            root_id: id,
            label: "Default".into(),
            passphrase_record: None,
        });
        Ok(identity)
    }
    pub fn import_wif(
        &mut self,
        label: &str,
        wif: &str,
        network: Network,
        password: Option<&str>,
    ) -> Result<Uuid> {
        if !self.data.settings.mode.can_sign() {
            return Err(Error::Mode);
        }
        if !network.is_bitcoin() {
            return Err(Error::Invalid("Bitcoin WIF network"));
        }
        let key = PrivateKey::from_wif(wif).map_err(|_| Error::Invalid("invalid Bitcoin WIF"))?;
        if !key.compressed || key.network != network.native().into() {
            return Err(Error::Invalid("WIF network or compression mismatch"));
        }
        let label = model::label(label)?;
        let id = Uuid::new_v4();
        let record =
            self.encrypt_record(id, &*Zeroizing::new(key.inner.secret_bytes()), password)?;
        self.data.records.push(record);
        self.data.roots.push(Root {
            id,
            label: label.clone(),
            description: String::new(),
            created_at: model::now(),
            kind: "bitcoin_wif".into(),
            custom_password: password.is_some(),
            reauthenticate: true,
        });
        let identity = Uuid::new_v4();
        self.data.identities.push(Identity {
            id: identity,
            root_id: id,
            label: "Imported key".into(),
            passphrase_record: None,
        });
        self.derive_account(identity, &label, network, "", password)?;
        Ok(identity)
    }
    pub fn add_identity(
        &mut self,
        root_id: Uuid,
        label: &str,
        passphrase: &str,
        root_password: Option<&str>,
    ) -> Result<Uuid> {
        if !self.data.settings.mode.can_sign() {
            return Err(Error::Mode);
        }
        if passphrase.is_empty() || passphrase.len() > 1024 {
            return Err(Error::Invalid("passphrase must contain 1–1024 bytes"));
        }
        let root = self
            .data
            .roots
            .iter()
            .find(|r| r.id == root_id && r.kind == "bip39")
            .ok_or(Error::NotFound)?;
        self.decrypt_record(root_id, root_password)?;
        let label = model::label(label)?;
        let id = Uuid::new_v4();
        // Passphrases inherit the root protection, including custom-password isolation.
        let record = self.encrypt_record(
            id,
            passphrase.as_bytes(),
            if root.custom_password {
                root_password
            } else {
                None
            },
        )?;
        self.data.records.push(record);
        self.data.identities.push(Identity {
            id,
            root_id,
            label,
            passphrase_record: Some(id),
        });
        Ok(id)
    }
    pub fn derive_account(
        &mut self,
        identity_id: Uuid,
        label: &str,
        network: Network,
        path: &str,
        password: Option<&str>,
    ) -> Result<Uuid> {
        if !self.data.settings.mode.can_sign() {
            return Err(Error::Mode);
        }
        if path.starts_with("m/48'") {
            return Err(Error::Invalid(
                "register a BIP48 account through Bitcoin multisig",
            ));
        }
        let label = model::label(label)?;
        let (private, fingerprint) = self.derive_material(identity_id, network, path, password)?;
        let (address, public_key) = crate::chains::keys::public_account(&private, network, path)?;
        if self
            .data
            .accounts
            .iter()
            .any(|a| a.identity_id == Some(identity_id) && a.network == network && a.path == path)
        {
            return Err(Error::Duplicate);
        }
        let id = Uuid::new_v4();
        self.data.accounts.push(Account {
            multisig: None,
            id,
            identity_id: Some(identity_id),
            label,
            network,
            path: path.into(),
            address,
            public_key,
            fingerprint,
            watch_only: false,
            tokens: vec![],
        });
        Ok(id)
    }
    pub(crate) fn derive_secret(
        &self,
        identity_id: Uuid,
        network: Network,
        path: &str,
        password: Option<&str>,
    ) -> Result<(Zeroizing<[u8; 32]>, String)> {
        let identity = self
            .data
            .identities
            .iter()
            .find(|i| i.id == identity_id)
            .ok_or(Error::NotFound)?;
        let root = self
            .data
            .roots
            .iter()
            .find(|r| r.id == identity.root_id)
            .ok_or(Error::NotFound)?;
        let bytes = self.decrypt_record(root.id, password)?;
        if root.kind == "bitcoin_wif" {
            if !path.is_empty() {
                return Err(Error::Invalid("standalone keys have no derivation path"));
            }
            return Ok((
                Zeroizing::new(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| Error::Authentication)?,
                ),
                String::new(),
            ));
        }
        validate_path(path, network)?;
        let phrase = std::str::from_utf8(&bytes).map_err(|_| Error::Authentication)?;
        let mnemonic = Mnemonic::parse(phrase).map_err(|_| Error::Authentication)?;
        let passphrase = match identity.passphrase_record {
            Some(id) => self.decrypt_record(id, password)?,
            None => Zeroizing::new(vec![]),
        };
        let passphrase = std::str::from_utf8(&passphrase).map_err(|_| Error::Authentication)?;
        let seed = Zeroizing::new(mnemonic.to_seed(passphrase));
        let secp = Secp256k1::new();
        let mut master = Xpriv::new_master(network.native(), &*seed)
            .map_err(|_| Error::Invalid("BIP32 seed"))?;
        let fingerprint = master.fingerprint(&secp).to_string();
        let derivation =
            DerivationPath::from_str(path).map_err(|_| Error::Invalid("derivation path"))?;
        let mut child = master
            .derive_priv(&secp, &derivation)
            .map_err(|_| Error::Invalid("BIP32 path"))?;
        let result = Zeroizing::new(child.private_key.secret_bytes());
        master.private_key.non_secure_erase();
        child.private_key.non_secure_erase();
        Ok((result, fingerprint))
    }
    pub(crate) fn derive_material(
        &self,
        identity_id: Uuid,
        network: Network,
        path: &str,
        password: Option<&str>,
    ) -> Result<(Zeroizing<Vec<u8>>, String)> {
        if network.is_bitcoin() {
            let (key, fingerprint) = self.derive_secret(identity_id, network, path, password)?;
            return Ok((Zeroizing::new(key.to_vec()), fingerprint));
        }
        let identity = self
            .data
            .identities
            .iter()
            .find(|i| i.id == identity_id)
            .ok_or(Error::NotFound)?;
        let root = self
            .data
            .roots
            .iter()
            .find(|r| r.id == identity.root_id && r.kind == "bip39")
            .ok_or(Error::Invalid("this key is not a recovery phrase"))?;
        let phrase = self.decrypt_record(root.id, password)?;
        let mnemonic =
            Mnemonic::parse(std::str::from_utf8(&phrase).map_err(|_| Error::Authentication)?)
                .map_err(|_| Error::Authentication)?;
        let passphrase = match identity.passphrase_record {
            Some(id) => self.decrypt_record(id, password)?,
            None => Zeroizing::new(vec![]),
        };
        crate::chains::keys::derive(
            &mnemonic,
            std::str::from_utf8(&passphrase).map_err(|_| Error::Authentication)?,
            network,
            path,
        )
    }
    pub fn add_token(&mut self, account_id: Uuid, token: crate::model::Token) -> Result<()> {
        let account = self
            .data
            .accounts
            .iter_mut()
            .find(|a| a.id == account_id)
            .ok_or(Error::NotFound)?;
        token.validate(account.network)?;
        if account.tokens.len() >= 100 {
            return Err(Error::Invalid("token limit"));
        }
        if account.tokens.iter().any(|t| {
            if account.network.evm_id().is_some() {
                t.contract.eq_ignore_ascii_case(&token.contract)
            } else {
                t.contract == token.contract
            }
        }) {
            return Err(Error::Duplicate);
        }
        account.tokens.push(token);
        Ok(())
    }
    pub fn verify_recovery(
        &self,
        root_id: Uuid,
        phrase: &str,
        password: Option<&str>,
    ) -> Result<bool> {
        use subtle::ConstantTimeEq;
        let entered = entropy::validate(phrase)?;
        let stored = self.decrypt_record(root_id, password)?;
        Ok(bool::from(entered.as_bytes().ct_eq(&stored)))
    }
    pub fn public_accounts(&self) -> Vec<Account> {
        self.data
            .accounts
            .iter()
            .cloned()
            .map(|mut a| {
                a.identity_id = None;
                a.watch_only = true;
                a
            })
            .collect()
    }
}
pub fn validate_path(path: &str, network: Network) -> Result<()> {
    if path.starts_with("m/48'") {
        let parts: Vec<_> = path.rsplitn(3, '/').collect();
        if parts.len() != 3 {
            return Err(Error::Invalid("BIP48 address path"));
        }
        crate::multisig::validate_origin(parts[2], network)?;
        let branch: u32 = parts[1]
            .parse()
            .map_err(|_| Error::Invalid("BIP48 branch"))?;
        let index: u32 = parts[0]
            .parse()
            .map_err(|_| Error::Invalid("BIP48 address index"))?;
        if branch > 1 || index >= (1 << 31) {
            return Err(Error::Invalid("BIP48 address index"));
        }
        return Ok(());
    }

    let parts: Vec<_> = path.split('/').collect();
    let coin = if network == Network::Bitcoin {
        "0'"
    } else {
        "1'"
    };
    if parts.len() != 6
        || parts[0] != "m"
        || !["44'", "49'", "84'", "86'"].contains(&parts[1])
        || parts[2] != coin
        || !parts[3].ends_with('\'')
        || !["0", "1"].contains(&parts[4])
    {
        return Err(Error::Invalid("use m/44',49',84',86'/coin'/account'/0-or-1/address with the correct network coin type"));
    }
    let derivation =
        DerivationPath::from_str(path).map_err(|_| Error::Invalid("derivation path"))?;
    if derivation.as_ref()[4].is_hardened() {
        return Err(Error::Invalid("address index must not be hardened"));
    }
    Ok(())
}
pub fn public_account(secret: &[u8; 32], network: Network, path: &str) -> Result<(String, String)> {
    let secp = Secp256k1::new();
    let mut key = SecretKey::from_slice(secret).map_err(|_| Error::Invalid("private key"))?;
    let public = PublicKey::new(key.public_key(&secp));
    let compressed = CompressedPublicKey(public.inner);
    let address = if path.starts_with("m/44'") {
        Address::p2pkh(public, network.native())
    } else if path.starts_with("m/49'") {
        Address::p2shwpkh(&compressed, network.native())
    } else if path.starts_with("m/86'") {
        Address::p2tr(
            &secp,
            public.inner.x_only_public_key().0,
            None,
            network.native(),
        )
    } else {
        Address::p2wpkh(&compressed, network.native())
    };
    key.non_secure_erase();
    Ok((address.to_string(), public.to_string()))
}
