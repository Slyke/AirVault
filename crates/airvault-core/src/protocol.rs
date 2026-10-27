use crate::{
    model::{Account, Network},
    Error, Result,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
pub const MAX_EXCHANGE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Payload {
    pub encoding: String,
    pub data: String,
}
impl Payload {
    pub fn new(bytes: &[u8]) -> Self {
        Self {
            encoding: "base64".into(),
            data: STANDARD.encode(bytes),
        }
    }
    pub fn decode(&self) -> Result<Vec<u8>> {
        if self.encoding != "base64" || self.data.len() > MAX_PAYLOAD_BYTES * 4 / 3 + 4 {
            return Err(Error::Invalid("payload encoding or size"));
        }
        STANDARD
            .decode(&self.data)
            .map_err(|_| Error::Invalid("base64 payload"))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub format: String,
    pub version: u32,
    pub request_id: Uuid,
    pub kind: String,
    pub network: Network,
    pub payload: Payload,
    pub payload_sha256: String,
    pub description: String,
}
impl Request {
    pub fn bitcoin(network: Network, psbt: &[u8], description: String) -> Self {
        Self {
            format: "airvault-request".into(),
            version: 1,
            request_id: Uuid::new_v4(),
            kind: "bitcoin-psbt".into(),
            network,
            payload: Payload::new(psbt),
            payload_sha256: digest(psbt),
            description,
        }
    }
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_EXCHANGE_BYTES {
            return Err(Error::Invalid("request size"));
        }
        let request: Self = serde_json::from_slice(bytes)?;
        request.validate()?;
        Ok(request)
    }
    pub fn validate(&self) -> Result<Vec<u8>> {
        if self.format != "airvault-request"
            || self.version != 1
            || !["bitcoin-psbt", "chain-transaction"].contains(&self.kind.as_str())
        {
            return Err(Error::Unsupported("request format, version or operation"));
        }
        if self.description.len() > 512 {
            return Err(Error::Invalid("description length"));
        }
        let bytes = self.payload.decode()?;
        if digest(&bytes) != self.payload_sha256 {
            return Err(Error::Invalid("payload checksum mismatch"));
        }
        Ok(bytes)
    }
    pub fn binding(&self) -> Result<String> {
        self.validate()?;
        Ok(digest(&serde_json::to_vec(self)?))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Response {
    pub format: String,
    pub version: u32,
    pub request_id: Uuid,
    pub request_digest: String,
    pub network: Network,
    pub payload: Payload,
    pub payload_sha256: String,
    pub txid: String,
}
impl Response {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_EXCHANGE_BYTES {
            return Err(Error::Invalid("response size"));
        }
        let response: Self = serde_json::from_slice(bytes)?;
        if response.format != "airvault-signature"
            || response.version != 1
            || digest(&response.payload.decode()?) != response.payload_sha256
        {
            return Err(Error::Invalid("signature envelope"));
        }
        Ok(response)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PublicBundle {
    pub format: String,
    pub version: u32,
    pub accounts: Vec<Account>,
    pub checksum: String,
}
impl PublicBundle {
    pub fn new(accounts: Vec<Account>) -> Result<Self> {
        let checksum = digest(&serde_json::to_vec(&accounts)?);
        Ok(Self {
            format: "airvault-public".into(),
            version: 1,
            accounts,
            checksum,
        })
    }
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_EXCHANGE_BYTES {
            return Err(Error::Invalid("public bundle size"));
        }
        let bundle: Self = serde_json::from_slice(bytes)?;
        if bundle.format != "airvault-public"
            || bundle.version != 1
            || bundle.accounts.len() > 1000
            || bundle.checksum != digest(&serde_json::to_vec(&bundle.accounts)?)
        {
            return Err(Error::Invalid("public bundle"));
        }
        for a in &bundle.accounts {
            if a.identity_id.is_some() || !a.watch_only {
                return Err(Error::Invalid(
                    "public account contains a local signing reference",
                ));
            }
            crate::model::label(&a.label)?;
            if a.tokens.len() > 100 {
                return Err(Error::Invalid("token limit"));
            }
            for token in &a.tokens {
                token.validate(a.network)?;
            }
            if let Some(policy) = &a.multisig {
                policy.validate_account(a)?;
                continue;
            }
            if !a.network.is_bitcoin() {
                crate::chains::keys::validate_public(a)?;
                continue;
            }
            let key = a
                .public_key
                .parse::<::bitcoin::PublicKey>()
                .map_err(|_| Error::Invalid("public key"))?;
            let address = a
                .address
                .parse::<::bitcoin::Address<_>>()
                .map_err(|_| Error::Invalid("address"))?
                .require_network(a.network.native())
                .map_err(|_| Error::Invalid("address network"))?;
            if a.path.len() > 120 || a.fingerprint.len() > 8 {
                return Err(Error::Invalid("account metadata"));
            }
            if !key.compressed {
                return Err(Error::Invalid("compressed public key required"));
            }
            if !a.path.is_empty() {
                crate::wallet::validate_path(&a.path, a.network)?;
            }
            let compressed = ::bitcoin::CompressedPublicKey(key.inner);
            let expected = if a.path.starts_with("m/44'") {
                ::bitcoin::Address::p2pkh(key, a.network.native())
            } else if a.path.starts_with("m/49'") {
                ::bitcoin::Address::p2shwpkh(&compressed, a.network.native())
            } else if a.path.starts_with("m/86'") {
                ::bitcoin::Address::p2tr(
                    &::bitcoin::secp256k1::Secp256k1::verification_only(),
                    key.inner.x_only_public_key().0,
                    None,
                    a.network.native(),
                )
            } else {
                ::bitcoin::Address::p2wpkh(&compressed, a.network.native())
            };
            if address != expected {
                return Err(Error::Invalid("public key and address mismatch"));
            }
        }
        Ok(bundle)
    }
}
pub fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
