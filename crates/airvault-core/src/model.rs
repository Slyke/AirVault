use crate::storage::SecretRecord;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Offline,
    Proxy,
    Combined,
}
impl Mode {
    pub fn can_sign(self) -> bool {
        self != Self::Proxy
    }
    pub fn can_network(self) -> bool {
        self != Self::Offline && cfg!(feature = "network")
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Network {
    Bitcoin,
    Testnet,
    Testnet4,
    Signet,
    Regtest,
    Ethereum,
    Sepolia,
    Bnb,
    Polygon,
    Avalanche,
    Arbitrum,
    Optimism,
    Base,
    Litecoin,
    Dogecoin,
    BitcoinCash,
    Solana,
    SolanaDevnet,
    Polkadot,
    Cardano,
    CardanoPreprod,
    Cosmos,
    Xrp,
    Tron,
    TronNile,
}
impl Network {
    pub fn native(self) -> ::bitcoin::Network {
        match self {
            Self::Bitcoin => ::bitcoin::Network::Bitcoin,
            Self::Testnet => ::bitcoin::Network::Testnet,
            Self::Testnet4 => ::bitcoin::Network::Testnet4,
            Self::Signet => ::bitcoin::Network::Signet,
            Self::Regtest => ::bitcoin::Network::Regtest,
            _ => ::bitcoin::Network::Bitcoin,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Root {
    pub id: Uuid,
    pub label: String,
    pub description: String,
    pub created_at: u64,
    pub kind: String,
    pub custom_password: bool,
    pub reauthenticate: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Identity {
    pub id: Uuid,
    pub root_id: Uuid,
    pub label: String,
    pub passphrase_record: Option<Uuid>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Account {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multisig: Option<crate::multisig::Policy>,
    pub id: Uuid,
    pub identity_id: Option<Uuid>,
    pub label: String,
    pub network: Network,
    pub path: String,
    pub address: String,
    pub public_key: String,
    pub fingerprint: String,
    pub watch_only: bool,
    #[serde(default)]
    pub tokens: Vec<Token>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Token {
    pub standard: String,
    pub contract: String,
    pub symbol: String,
    pub decimals: u8,
}
impl Token {
    pub fn validate(&self, network: Network) -> crate::Result<()> {
        let expected = network.chain().tokens.to_ascii_lowercase().replace('-', "");
        if self.standard != expected
            || expected.is_empty()
            || self.symbol.is_empty()
            || self.symbol.len() > 16
            || !self.symbol.bytes().all(|b| b.is_ascii_alphanumeric())
            || self.decimals > 36
            || (expected == "spl" && self.decimals > 18)
        {
            return Err(crate::Error::Invalid("token standard, symbol or decimals"));
        }
        crate::chains::keys::address_bytes(&self.contract, network)?;
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct History {
    pub id: Uuid,
    pub request_id: Uuid,
    pub action: String,
    pub at: u64,
    pub digest: String,
    pub txid: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Settings {
    pub mode: Mode,
    pub timeout_seconds: i64,
    pub lock_on_blur: bool,
    pub rpc_url: String,
    #[serde(default)]
    pub providers: std::collections::BTreeMap<String, String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Offline,
            timeout_seconds: 300,
            lock_on_blur: false,
            rpc_url: String::new(),
            providers: Default::default(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VaultData {
    #[serde(default)]
    pub psbts: Vec<crate::multisig_psbt::Progress>,
    pub settings: Settings,
    pub roots: Vec<Root>,
    pub identities: Vec<Identity>,
    pub accounts: Vec<Account>,
    pub records: Vec<SecretRecord>,
    pub history: Vec<History>,
    pub pending: Vec<crate::protocol::Request>,
    pub responses: Vec<crate::protocol::Response>,
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn label(value: &str) -> crate::Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 120 || value.chars().any(char::is_control) {
        return Err(crate::Error::Invalid(
            "label must contain 1–120 printable characters",
        ));
    }
    Ok(value.to_owned())
}

impl Settings {
    pub fn provider(&self, network: Network) -> String {
        let key = serde_json::to_value(network)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        self.providers
            .get(&key)
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if network.is_bitcoin() {
                    self.rpc_url.clone()
                } else {
                    String::new()
                }
            })
    }
}
