//! Chain identity, address codecs, and deterministic key derivation.
//! No network access and no application logging.
pub mod keys;
pub mod transactions;
use crate::{model::Network, Error, Result};
use serde::Serialize;
use tw_coin_entry::{coin_context::CoinContext, derivation::DerivationWithPath};
use tw_keypair::tw::PublicKeyType;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chain {
    pub network: Network,
    pub name: &'static str,
    pub symbol: &'static str,
    pub decimals: u8,
    pub path: &'static str,
    pub tokens: &'static str,
}
impl Network {
    pub fn is_bitcoin(self) -> bool {
        matches!(
            self,
            Self::Bitcoin | Self::Testnet | Self::Testnet4 | Self::Signet | Self::Regtest
        )
    }
    pub fn is_utxo(self) -> bool {
        self.is_bitcoin() || matches!(self, Self::Litecoin | Self::Dogecoin | Self::BitcoinCash)
    }
    pub fn evm_id(self) -> Option<u64> {
        match self {
            Self::Ethereum => Some(1),
            Self::Sepolia => Some(11155111),
            Self::Bnb => Some(56),
            Self::Polygon => Some(137),
            Self::Avalanche => Some(43114),
            Self::Arbitrum => Some(42161),
            Self::Optimism => Some(10),
            Self::Base => Some(8453),
            _ => None,
        }
    }
    pub fn coin_type(self) -> u32 {
        if self.evm_id().is_some() {
            return 60;
        }
        match self {
            Self::Bitcoin => 0,
            Self::Testnet | Self::Testnet4 | Self::Signet | Self::Regtest => 1,
            Self::Litecoin => 2,
            Self::Dogecoin => 3,
            Self::BitcoinCash => 145,
            Self::Solana | Self::SolanaDevnet => 501,
            Self::Polkadot => 354,
            Self::Cardano | Self::CardanoPreprod => 1815,
            Self::Cosmos => 118,
            Self::Xrp => 144,
            Self::Tron | Self::TronNile => 195,
            _ => 60,
        }
    }
    pub fn chain(self) -> Chain {
        let (name, symbol, decimals, path, tokens) = match self {
            Self::Bitcoin => ("Bitcoin", "BTC", 8, "m/84'/0'/0'/0/0", ""),
            Self::Testnet => ("Bitcoin Testnet3", "tBTC", 8, "m/84'/1'/0'/0/0", ""),
            Self::Testnet4 => ("Bitcoin Testnet4", "tBTC", 8, "m/84'/1'/0'/0/0", ""),
            Self::Signet => ("Bitcoin Signet", "tBTC", 8, "m/84'/1'/0'/0/0", ""),
            Self::Regtest => ("Bitcoin Regtest", "tBTC", 8, "m/84'/1'/0'/0/0", ""),
            Self::Ethereum => ("Ethereum", "ETH", 18, "m/44'/60'/0'/0/0", "ERC-20"),
            Self::Sepolia => ("Ethereum Sepolia", "ETH", 18, "m/44'/60'/0'/0/0", "ERC-20"),
            Self::Bnb => ("BNB Smart Chain", "BNB", 18, "m/44'/60'/0'/0/0", "ERC-20"),
            Self::Polygon => ("Polygon", "POL", 18, "m/44'/60'/0'/0/0", "ERC-20"),
            Self::Avalanche => (
                "Avalanche C-Chain",
                "AVAX",
                18,
                "m/44'/60'/0'/0/0",
                "ERC-20",
            ),
            Self::Arbitrum => ("Arbitrum", "ETH", 18, "m/44'/60'/0'/0/0", "ERC-20"),
            Self::Optimism => ("Optimism", "ETH", 18, "m/44'/60'/0'/0/0", "ERC-20"),
            Self::Base => ("Base", "ETH", 18, "m/44'/60'/0'/0/0", "ERC-20"),
            Self::Litecoin => ("Litecoin", "LTC", 8, "m/44'/2'/0'/0/0", ""),
            Self::Dogecoin => ("Dogecoin", "DOGE", 8, "m/44'/3'/0'/0/0", ""),
            Self::BitcoinCash => ("Bitcoin Cash", "BCH", 8, "m/44'/145'/0'/0/0", ""),
            Self::Solana => ("Solana", "SOL", 9, "m/44'/501'/0'/0'", "SPL"),
            Self::SolanaDevnet => ("Solana Devnet", "SOL", 9, "m/44'/501'/0'/0'", "SPL"),
            Self::Polkadot => ("Polkadot", "DOT", 10, "", ""),
            Self::Cardano => ("Cardano", "ADA", 6, "m/1852'/1815'/0'/0/0", ""),
            Self::CardanoPreprod => ("Cardano Preprod", "ADA", 6, "m/1852'/1815'/0'/0/0", ""),
            Self::Cosmos => ("Cosmos Hub", "ATOM", 6, "m/44'/118'/0'/0/0", ""),
            Self::Xrp => ("XRP Ledger", "XRP", 6, "m/44'/144'/0'/0/0", ""),
            Self::Tron => ("Tron", "TRX", 6, "m/44'/195'/0'/0/0", "TRC-20"),
            Self::TronNile => ("Tron Nile", "TRX", 6, "m/44'/195'/0'/0/0", "TRC-20"),
        };
        Chain {
            network: self,
            name,
            symbol,
            decimals,
            path,
            tokens,
        }
    }
}
pub fn registry() -> Vec<Chain> {
    use Network::*;
    [
        Bitcoin,
        Ethereum,
        Dogecoin,
        Solana,
        Polkadot,
        Litecoin,
        BitcoinCash,
        Bnb,
        Polygon,
        Avalanche,
        Arbitrum,
        Optimism,
        Base,
        Cardano,
        Cosmos,
        Xrp,
        Tron,
        Testnet4,
        Testnet,
        Signet,
        Regtest,
        Sepolia,
        SolanaDevnet,
        CardanoPreprod,
        TronNile,
    ]
    .into_iter()
    .map(Network::chain)
    .collect()
}
pub struct Context(pub Network);
impl CoinContext for Context {
    fn public_key_type(&self) -> PublicKeyType {
        if matches!(self.0, Network::Solana | Network::SolanaDevnet) {
            PublicKeyType::Ed25519
        } else {
            PublicKeyType::Secp256k1
        }
    }
    fn address_hasher(&self) -> Option<tw_hash::hasher::Hasher> {
        None
    }
    fn hrp(&self) -> Option<String> {
        Some(
            match self.0 {
                Network::Cosmos => "cosmos",
                Network::BitcoinCash => "bitcoincash",
                Network::Litecoin => "ltc",
                _ => "bc",
            }
            .into(),
        )
    }
    fn p2pkh_prefix(&self) -> Option<u8> {
        Some(match self.0 {
            Network::Litecoin => 48,
            Network::Dogecoin => 30,
            _ => 0,
        })
    }
    fn p2sh_prefix(&self) -> Option<u8> {
        Some(match self.0 {
            Network::Litecoin => 50,
            Network::Dogecoin => 22,
            _ => 5,
        })
    }
    fn ss58_prefix(&self) -> Option<u16> {
        Some(0)
    }
    fn derivations(&self) -> &[DerivationWithPath] {
        &[]
    }
}
pub fn invalid<T>(_: T) -> Error {
    Error::Invalid("invalid chain key, address or transaction field")
}
pub fn bytes<const N: usize>(value: &[u8]) -> Result<[u8; N]> {
    value.try_into().map_err(invalid)
}
pub fn hex_bytes(value: &str) -> Result<Vec<u8>> {
    if value.len() > 4_194_304 {
        return Err(Error::Invalid("hex data too large"));
    }
    hex::decode(value.strip_prefix("0x").unwrap_or(value)).map_err(invalid)
}
pub fn units(value: &str) -> Result<u128> {
    if value.is_empty()
        || value.len() > 39
        || !value.bytes().all(|b| b.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(Error::Invalid(
            "use a whole, non-negative number of base units",
        ));
    }
    value.parse().map_err(invalid)
}
pub fn display_units(value: u128, decimals: u8) -> String {
    let divisor = 10u128.pow(decimals as u32);
    let whole = value / divisor;
    let fraction = value % divisor;
    if fraction == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{fraction:0width$}", width = decimals as usize)
            .trim_end_matches('0')
            .to_string()
    }
}
