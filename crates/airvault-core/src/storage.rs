use crate::{model::VaultData, Error, Result};
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::Path,
};
use uuid::Uuid;
use zeroize::Zeroizing;

pub const MAX_VAULT_BYTES: u64 = 32 * 1024 * 1024;
const MEMORY_KIB: u32 = 65536;
const ITERATIONS: u32 = 3;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Sealed {
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PasswordWrap {
    salt: Vec<u8>,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    sealed: Sealed,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum KeyWrap {
    Vault { sealed: Sealed },
    Password { wrapped: PasswordWrap },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SecretRecord {
    pub id: Uuid,
    pub wrapped_key: KeyWrap,
    pub secret: Sealed,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VaultFile {
    format: String,
    version: u32,
    pub id: Uuid,
    master: PasswordWrap,
    metadata: Sealed,
}
pub struct OpenVault {
    pub file: VaultFile,
    master_key: Zeroizing<Vec<u8>>,
    pub data: VaultData,
}
fn random(len: usize) -> Result<Zeroizing<Vec<u8>>> {
    let mut value = Zeroizing::new(vec![0; len]);
    OsRng
        .try_fill_bytes(&mut value)
        .map_err(|_| Error::Invalid("OS randomness unavailable"))?;
    Ok(value)
}
fn seal(key: &[u8], plaintext: &[u8], aad: &[u8]) -> Result<Sealed> {
    let nonce = random(12)?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| Error::Authentication)?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| Error::Authentication)?;
    Ok(Sealed {
        nonce: nonce.to_vec(),
        ciphertext,
    })
}
fn open(key: &[u8], sealed: &Sealed, aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    if sealed.nonce.len() != 12 || sealed.ciphertext.len() > MAX_VAULT_BYTES as usize {
        return Err(Error::Authentication);
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| Error::Authentication)?;
    cipher
        .decrypt(
            Nonce::from_slice(&sealed.nonce),
            Payload {
                msg: &sealed.ciphertext,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| Error::Authentication)
}
fn derive(password: &str, wrap: &PasswordWrap) -> Result<Zeroizing<Vec<u8>>> {
    if password.len() > 1024
        || wrap.salt.len() != 16
        || wrap.memory_kib != MEMORY_KIB
        || wrap.iterations != ITERATIONS
        || wrap.parallelism != 1
    {
        return Err(Error::Authentication);
    }
    let params =
        Params::new(MEMORY_KIB, ITERATIONS, 1, Some(32)).map_err(|_| Error::Authentication)?;
    let mut key = Zeroizing::new(vec![0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), &wrap.salt, &mut key)
        .map_err(|_| Error::Authentication)?;
    Ok(key)
}
fn password_wrap(password: &str, key: &[u8], aad: &[u8]) -> Result<PasswordWrap> {
    if password.chars().count() < 6 || password.len() > 1024 {
        return Err(Error::Invalid("use a password of at least 6 characters"));
    }
    let mut wrap = PasswordWrap {
        salt: random(16)?.to_vec(),
        memory_kib: MEMORY_KIB,
        iterations: ITERATIONS,
        parallelism: 1,
        sealed: Sealed {
            nonce: vec![],
            ciphertext: vec![],
        },
    };
    wrap.sealed = seal(&derive(password, &wrap)?, key, aad)?;
    Ok(wrap)
}
fn aad(id: Uuid, role: &str) -> String {
    format!("airvault:v1:{id}:{role}")
}

impl OpenVault {
    pub fn create(password: &str, data: VaultData) -> Result<Self> {
        let id = Uuid::new_v4();
        let master_key = random(32)?;
        let master = password_wrap(password, &master_key, aad(id, "master").as_bytes())?;
        let metadata = seal(
            &master_key,
            &Zeroizing::new(serde_json::to_vec(&data)?),
            aad(id, "metadata").as_bytes(),
        )?;
        Ok(Self {
            file: VaultFile {
                format: "airvault-vault".into(),
                version: 1,
                id,
                master,
                metadata,
            },
            master_key,
            data,
        })
    }
    pub fn unlock(bytes: &[u8], password: &str) -> Result<Self> {
        if bytes.len() > MAX_VAULT_BYTES as usize {
            return Err(Error::Invalid("vault exceeds size limit"));
        }
        let file: VaultFile = serde_json::from_slice(bytes)?;
        if file.format != "airvault-vault" || file.version != 1 {
            return Err(Error::Unsupported("vault format/version"));
        }
        let kek = derive(password, &file.master)?;
        let master_key = open(&kek, &file.master.sealed, aad(file.id, "master").as_bytes())?;
        let metadata = open(
            &master_key,
            &file.metadata,
            aad(file.id, "metadata").as_bytes(),
        )?;
        let data: VaultData = serde_json::from_slice(&metadata)?;
        Ok(Self {
            file,
            master_key,
            data,
        })
    }
    pub fn authenticate(&self, password: &str) -> Result<()> {
        let kek = derive(password, &self.file.master)?;
        let candidate = open(
            &kek,
            &self.file.master.sealed,
            aad(self.file.id, "master").as_bytes(),
        )?;
        use subtle::ConstantTimeEq;
        if !bool::from(candidate.ct_eq(&self.master_key)) {
            return Err(Error::Authentication);
        }
        Ok(())
    }
    pub fn change_password(&mut self, old: &str, new: &str) -> Result<()> {
        self.authenticate(old)?;
        self.file.master = password_wrap(
            new,
            &self.master_key,
            aad(self.file.id, "master").as_bytes(),
        )?;
        Ok(())
    }
    pub fn encrypt_record(
        &self,
        id: Uuid,
        plaintext: &[u8],
        password: Option<&str>,
    ) -> Result<SecretRecord> {
        let record_key = random(32)?;
        let wrapped_key = match password {
            Some(password) => KeyWrap::Password {
                wrapped: password_wrap(password, &record_key, aad(id, "record-key").as_bytes())?,
            },
            None => KeyWrap::Vault {
                sealed: seal(
                    &self.master_key,
                    &record_key,
                    aad(id, "record-key").as_bytes(),
                )?,
            },
        };
        Ok(SecretRecord {
            id,
            wrapped_key,
            secret: seal(&record_key, plaintext, aad(id, "secret").as_bytes())?,
        })
    }
    fn record_key(
        &self,
        record: &SecretRecord,
        password: Option<&str>,
    ) -> Result<Zeroizing<Vec<u8>>> {
        match &record.wrapped_key {
            KeyWrap::Vault { sealed } => open(
                &self.master_key,
                sealed,
                aad(record.id, "record-key").as_bytes(),
            ),
            KeyWrap::Password { wrapped } => {
                let kek = derive(password.ok_or(Error::Authentication)?, wrapped)?;
                open(
                    &kek,
                    &wrapped.sealed,
                    aad(record.id, "record-key").as_bytes(),
                )
            }
        }
    }
    pub fn decrypt_record(&self, id: Uuid, password: Option<&str>) -> Result<Zeroizing<Vec<u8>>> {
        let record = self
            .data
            .records
            .iter()
            .find(|r| r.id == id)
            .ok_or(Error::NotFound)?;
        open(
            &self.record_key(record, password)?,
            &record.secret,
            aad(id, "secret").as_bytes(),
        )
    }
    pub fn rewrap_record(&mut self, id: Uuid, old: Option<&str>, new: Option<&str>) -> Result<()> {
        let index = self
            .data
            .records
            .iter()
            .position(|r| r.id == id)
            .ok_or(Error::NotFound)?;
        let key = self.record_key(&self.data.records[index], old)?;
        self.data.records[index].wrapped_key = match new {
            Some(p) => KeyWrap::Password {
                wrapped: password_wrap(p, &key, aad(id, "record-key").as_bytes())?,
            },
            None => KeyWrap::Vault {
                sealed: seal(&self.master_key, &key, aad(id, "record-key").as_bytes())?,
            },
        };
        Ok(())
    }
    pub fn encrypted_bytes(&mut self) -> Result<Vec<u8>> {
        let bytes = Zeroizing::new(serde_json::to_vec(&self.data)?);
        self.file.metadata = seal(
            &self.master_key,
            &bytes,
            aad(self.file.id, "metadata").as_bytes(),
        )?;
        let encoded = serde_json::to_vec(&self.file)?;
        if encoded.len() as u64 > MAX_VAULT_BYTES {
            return Err(Error::Invalid("vault capacity reached"));
        }
        Ok(encoded)
    }
    pub fn save(&mut self, path: &Path) -> Result<()> {
        atomic_write(path, &self.encrypted_bytes()?)
    }
}
pub fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > max {
        return Err(Error::Invalid("file exceeds size limit"));
    }
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(Error::Invalid("file exceeds size limit"));
    }
    Ok(bytes)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or(Error::Storage)?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|_| Error::Storage)?;
    #[cfg(unix)]
    {
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}
