//! Registered BIP48 native SegWit sortedmulti wallets. Public configuration is recovery data.
use crate::{
    model::{Account, Network},
    storage::OpenVault,
    Error, Result,
};
use bitcoin::{
    bip32::{ChildNumber, DerivationPath, Fingerprint, Xpriv, Xpub},
    opcodes::all::OP_CHECKMULTISIG,
    script::Builder,
    secp256k1::Secp256k1,
    Address, PublicKey, ScriptBuf,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, str::FromStr};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Cosigner {
    pub fingerprint: String,
    pub path: String,
    pub xpub: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Policy {
    pub threshold: u8,
    pub cosigners: Vec<Cosigner>,
    pub branch: u32,
    pub index: u32,
}
impl Cosigner {
    pub fn parse(value: &str) -> Result<Self> {
        if value.len() > 250 {
            return Err(Error::Invalid("cosigner length"));
        }
        let (origin, xpub) = value
            .trim()
            .strip_prefix('[')
            .and_then(|s| s.split_once(']'))
            .ok_or(Error::Invalid(
                "use [fingerprint/48h/coinh/accounth/2h]xpub",
            ))?;
        let (fingerprint, path) = origin
            .split_once('/')
            .ok_or(Error::Invalid("cosigner origin"))?;
        Ok(Self {
            fingerprint: fingerprint.to_lowercase(),
            path: format!("m/{}", path.replace('h', "'")),
            xpub: xpub.into(),
        })
    }
    pub fn text(&self) -> String {
        format!(
            "[{}/{}]{}",
            self.fingerprint,
            self.path.trim_start_matches("m/").replace('\'', "h"),
            self.xpub
        )
    }
}
pub fn account_path(network: Network, account: u32) -> Result<String> {
    if !network.is_bitcoin() || account >= (1 << 31) {
        return Err(Error::Invalid("Bitcoin account index"));
    }
    Ok(format!(
        "m/48'/{}'/{}'/2'",
        if network == Network::Bitcoin { 0 } else { 1 },
        account
    ))
}
pub fn validate_origin(path: &str, network: Network) -> Result<DerivationPath> {
    let parsed = DerivationPath::from_str(path).map_err(|_| Error::Invalid("BIP48 origin"))?;
    let parts: Vec<_> = parsed.into_iter().copied().collect();
    if parts.len() != 4 || !parts.iter().all(|p| p.is_hardened()) {
        return Err(Error::Invalid("BIP48 origin"));
    }
    let account = u32::from(parts[2]) & 0x7fff_ffff;
    if path != account_path(network, account)? {
        return Err(Error::Invalid("use BIP48 native SegWit account origins"));
    }
    Ok(parsed)
}
impl Policy {
    pub fn validate(&self, network: Network) -> Result<()> {
        if !network.is_bitcoin()
            || self.cosigners.len() < 2
            || self.cosigners.len() > 15
            || self.threshold == 0
            || self.threshold as usize > self.cosigners.len()
            || self.branch > 1
            || self.index >= (1 << 31)
        {
            return Err(Error::Invalid("multisig requires 2–15 cosigners, a valid threshold, and an unhardened address index"));
        }
        let mut keys = HashSet::new();
        let mut roots = HashSet::new();
        for c in &self.cosigners {
            if c.fingerprint.len() != 8 || c.path.len() > 80 || c.xpub.len() > 120 {
                return Err(Error::Invalid("cosigner metadata"));
            }
            Fingerprint::from_str(&c.fingerprint)
                .map_err(|_| Error::Invalid("cosigner fingerprint"))?;
            let path = validate_origin(&c.path, network)?;
            let xpub = Xpub::from_str(&c.xpub).map_err(|_| Error::Invalid("cosigner xpub"))?;
            if xpub.network != network.native().into()
                || xpub.depth != 4
                || xpub.child_number != *path.into_iter().last().ok_or(Error::Verification)?
            {
                return Err(Error::Invalid("xpub network or origin depth"));
            }
            // Reusing a root is not an independent signer, even at a different account index.
            if !keys.insert(xpub.public_key) || !roots.insert(c.fingerprint.to_lowercase()) {
                return Err(Error::Invalid(
                    "cosigners must use distinct keys and root fingerprints",
                ));
            }
        }
        Ok(())
    }
    pub fn derived(
        &self,
        network: Network,
    ) -> Result<Vec<(PublicKey, (Fingerprint, DerivationPath))>> {
        self.validate(network)?;
        let secp = Secp256k1::verification_only();
        let tail = [
            ChildNumber::from_normal_idx(self.branch).map_err(|_| Error::Verification)?,
            ChildNumber::from_normal_idx(self.index).map_err(|_| Error::Verification)?,
        ];
        let mut keys = vec![];
        for c in &self.cosigners {
            let xpub = Xpub::from_str(&c.xpub)
                .map_err(|_| Error::Verification)?
                .derive_pub(&secp, &tail)
                .map_err(|_| Error::Invalid("public derivation"))?;
            let path = validate_origin(&c.path, network)?.extend(tail);
            keys.push((
                PublicKey::new(xpub.public_key),
                (
                    Fingerprint::from_str(&c.fingerprint).map_err(|_| Error::Verification)?,
                    path,
                ),
            ));
        }
        keys.sort_by_key(|k| k.0.to_bytes());
        if keys.windows(2).any(|w| w[0].0 == w[1].0) {
            return Err(Error::Invalid("duplicate derived public key"));
        }
        Ok(keys)
    }
    pub fn script(&self, network: Network) -> Result<ScriptBuf> {
        let mut builder = Builder::new().push_int(self.threshold as i64);
        for (key, _) in self.derived(network)? {
            builder = builder.push_key(&key);
        }
        Ok(builder
            .push_int(self.cosigners.len() as i64)
            .push_opcode(OP_CHECKMULTISIG)
            .into_script())
    }
    pub fn address(&self, network: Network) -> Result<String> {
        Ok(Address::p2wsh(&self.script(network)?, network.native()).to_string())
    }
    pub fn descriptor(&self, branch: u32) -> String {
        // The backup includes explicit receive/change ranged descriptors (no secret keys).
        format!(
            "wsh(sortedmulti({},{}))",
            self.threshold,
            self.cosigners
                .iter()
                .map(|c| format!("{}/{branch}/*", c.text()))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
    pub fn validate_account(&self, account: &Account) -> Result<()> {
        if self.address(account.network)? != account.address || !account.tokens.is_empty() {
            return Err(Error::Verification);
        }
        Ok(())
    }
}

impl OpenVault {
    pub fn multisig_cosigner(
        &self,
        identity_id: Uuid,
        network: Network,
        account: u32,
        password: Option<&str>,
    ) -> Result<Cosigner> {
        if !self.data.settings.mode.can_sign() {
            return Err(Error::Mode);
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
            .ok_or(Error::Invalid(
                "multisig requires a recovery phrase identity",
            ))?;
        let bytes = self.decrypt_record(root.id, password)?;
        let mnemonic =
            bip39::Mnemonic::parse(std::str::from_utf8(&bytes).map_err(|_| Error::Authentication)?)
                .map_err(|_| Error::Authentication)?;
        let pass = match identity.passphrase_record {
            Some(id) => self.decrypt_record(id, password)?,
            None => Zeroizing::new(vec![]),
        };
        let seed = Zeroizing::new(
            mnemonic.to_seed(std::str::from_utf8(&pass).map_err(|_| Error::Authentication)?),
        );
        let path = account_path(network, account)?;
        let secp = Secp256k1::new();
        let mut master =
            Xpriv::new_master(network.native(), &*seed).map_err(|_| Error::Verification)?;
        let fingerprint = master.fingerprint(&secp).to_string();
        let derived = master.derive_priv(&secp, &validate_origin(&path, network)?);
        master.private_key.non_secure_erase();
        let mut child = derived.map_err(|_| Error::Verification)?;
        let xpub = Xpub::from_priv(&secp, &child).to_string();
        child.private_key.non_secure_erase();
        Ok(Cosigner {
            fingerprint,
            path,
            xpub,
        })
    }
    pub fn register_multisig(
        &mut self,
        label: &str,
        network: Network,
        policy: Policy,
        identity_id: Option<Uuid>,
        password: Option<&str>,
    ) -> Result<Uuid> {
        let label = crate::model::label(label)?;
        policy.validate(network)?;
        let mut path = String::new();
        let mut fingerprint = String::new();
        let mut public_key = String::new();
        if let Some(id) = identity_id {
            // Compare the complete derived xpub, not the short fingerprint.
            let mut matched = None;
            for c in &policy.cosigners {
                let origin = validate_origin(&c.path, network)?;
                let account = u32::from(origin.as_ref()[2]) & 0x7fff_ffff;
                let local = self.multisig_cosigner(id, network, account, password)?;
                if &local == c {
                    matched = Some(c);
                    break;
                }
            }
            let c = matched.ok_or(Error::Invalid(
                "selected identity is not a cosigner in this configuration",
            ))?;
            path = format!("{}/{}/{}", c.path, policy.branch, policy.index);
            fingerprint = c.fingerprint.clone();
            let (key, _) = self.derive_secret(id, network, &path, password)?;
            let mut secret = bitcoin::secp256k1::SecretKey::from_slice(&*key)
                .map_err(|_| Error::Verification)?;
            public_key = bitcoin::secp256k1::PublicKey::from_secret_key(&Secp256k1::new(), &secret)
                .to_string();
            secret.non_secure_erase();
        }
        let address = policy.address(network)?;
        if self
            .data
            .accounts
            .iter()
            .any(|a| a.network == network && a.address == address && a.identity_id == identity_id)
        {
            return Err(Error::Duplicate);
        }
        let id = Uuid::new_v4();
        self.data.accounts.push(Account {
            id,
            identity_id,
            label,
            network,
            path,
            address,
            public_key,
            fingerprint,
            watch_only: identity_id.is_none(),
            tokens: vec![],
            multisig: Some(policy),
        });
        Ok(id)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Backup {
    pub format: String,
    pub version: u32,
    pub label: String,
    pub network: Network,
    pub policy: Policy,
    pub address: String,
    pub receive_descriptor: String,
    pub change_descriptor: String,
    pub checksum: String,
}
impl Backup {
    pub fn new(label: &str, network: Network, policy: Policy) -> Result<Self> {
        policy.validate(network)?;
        let checksum = crate::protocol::digest(&serde_json::to_vec(&(network, &policy))?);
        Ok(Self {
            format: "airvault-multisig".into(),
            version: 1,
            label: crate::model::label(label)?,
            address: policy.address(network)?,
            receive_descriptor: descriptor_checksum(&policy.descriptor(0))?,
            change_descriptor: descriptor_checksum(&policy.descriptor(1))?,
            network,
            policy,
            checksum,
        })
    }
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 32768 {
            return Err(Error::Invalid("multisig configuration size"));
        }
        let backup: Self = serde_json::from_slice(bytes)?;
        let expected = Self::new(&backup.label, backup.network, backup.policy.clone())?;
        if backup.format != expected.format
            || backup.version != 1
            || backup.address != expected.address
            || backup.checksum != expected.checksum
            || backup.receive_descriptor != expected.receive_descriptor
            || backup.change_descriptor != expected.change_descriptor
        {
            return Err(Error::Invalid(
                "multisig configuration checksum or descriptor mismatch",
            ));
        }
        Ok(backup)
    }
}
// BIP380 descriptor checksum. Independently compared with Bitcoin Core in regtest.
pub fn descriptor_checksum(descriptor: &str) -> Result<String> {
    const INPUT: &str = "0123456789()[],'/*abcdefgh@:$%{}IJKLMNOPQRSTUVWXYZ&+-.;<=>?!^_|~ijklmnopqrstuvwxyzABCDEFGH`#\"\\ ";
    const OUTPUT: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    fn step(state: u64, value: u64) -> u64 {
        let high = state >> 35;
        let mut next = ((state & 0x7ffffffff) << 5) ^ value;
        for (bit, polynomial) in [
            0xf5dee51989,
            0xa9fdca3312,
            0x1bab10e32d,
            0x3706b1677a,
            0x644d626ffd,
        ]
        .iter()
        .enumerate()
        {
            if high & (1 << bit) != 0 {
                next ^= polynomial;
            }
        }
        next
    }
    let mut state = 1;
    let mut group = 0;
    let mut count = 0;
    for ch in descriptor.chars() {
        let position = INPUT
            .find(ch)
            .ok_or(Error::Invalid("descriptor character"))? as u64;
        state = step(state, position & 31);
        group = group * 3 + (position >> 5);
        count += 1;
        if count == 3 {
            state = step(state, group);
            group = 0;
            count = 0;
        }
    }
    if count != 0 {
        state = step(state, group);
    }
    for _ in 0..8 {
        state = step(state, 0);
    }
    state ^= 1;
    let checksum: String = (0..8)
        .map(|i| OUTPUT[((state >> (5 * (7 - i))) & 31) as usize] as char)
        .collect();
    Ok(format!("{descriptor}#{checksum}"))
}
