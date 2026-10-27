use super::{bytes, invalid, Context};
use crate::{
    model::{Account, Network},
    Error, Result,
};
use bip39::Mnemonic;
use bitcoin::{
    bip32::{DerivationPath, Xpriv},
    hashes::{hash160, Hash},
    secp256k1::{PublicKey, Secp256k1, SecretKey},
};
use cardano_serialization_lib as ada;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha512};
use sha3::Keccak256;
use std::str::FromStr;
use tw_coin_entry::{coin_entry::CoinEntry, derivation::Derivation};
use tw_keypair::tw::{PublicKey as TwPublic, PublicKeyType};
use zeroize::Zeroizing;

pub fn validate_path(path: &str, network: Network) -> Result<()> {
    if network.is_bitcoin() {
        return crate::wallet::validate_path(path, network);
    }
    if path.len() > 120 {
        return Err(Error::Invalid("derivation path too long"));
    }
    if network == Network::Polkadot {
        if path.is_empty() {
            return Ok(());
        }
        if !path.starts_with("//")
            || path[2..]
                .split("//")
                .any(|p| p.is_empty() || p.parse::<u64>().is_err())
        {
            return Err(Error::Invalid(
                "Polkadot path must be empty or numeric hard junctions, such as //0",
            ));
        }
        return Ok(());
    }
    let derivation = DerivationPath::from_str(path).map_err(invalid)?;
    let p: Vec<_> = path.split('/').collect();
    let purpose = if matches!(network, Network::Cardano | Network::CardanoPreprod) {
        "1852'"
    } else {
        "44'"
    };
    if p.len() < 4
        || p[0] != "m"
        || p[1] != purpose
        || p[2] != format!("{}'", network.coin_type())
        || !derivation.as_ref()[2].is_hardened()
    {
        return Err(Error::Invalid(
            "derivation path does not match the selected blockchain",
        ));
    }
    if matches!(network, Network::Solana | Network::SolanaDevnet) {
        if !(4..=6).contains(&p.len()) || derivation.as_ref().iter().any(|i| !i.is_hardened()) {
            return Err(Error::Invalid("Solana SLIP-0010 requires hardened indices"));
        }
    } else if p.len() != 6 || !["0", "1"].contains(&p[4]) || derivation.as_ref()[4].is_hardened() {
        return Err(Error::Invalid("use purpose/coin/account/0-or-1/address"));
    }
    Ok(())
}
pub fn derive(
    mnemonic: &Mnemonic,
    passphrase: &str,
    network: Network,
    path: &str,
) -> Result<(Zeroizing<Vec<u8>>, String)> {
    validate_path(path, network)?;
    if network == Network::Polkadot {
        let entropy = Zeroizing::new(mnemonic.to_entropy());
        let mut key =
            substrate_bip39::mini_secret_from_entropy(&entropy, passphrase).map_err(invalid)?;
        if !path.is_empty() {
            for index in path[2..].split("//") {
                let mut chain = [0u8; 32];
                chain[..8].copy_from_slice(&index.parse::<u64>().map_err(invalid)?.to_le_bytes());
                key = key
                    .hard_derive_mini_secret_key(
                        Some(schnorrkel::derive::ChainCode(chain)),
                        b"",
                        schnorrkel::ExpansionMode::Ed25519,
                    )
                    .0;
            }
        }
        return Ok((Zeroizing::new(key.to_bytes().to_vec()), String::new()));
    }
    let derivation = DerivationPath::from_str(path).map_err(invalid)?;
    if matches!(network, Network::Cardano | Network::CardanoPreprod) {
        let entropy = Zeroizing::new(mnemonic.to_entropy());
        let mut master = ada::Bip32PrivateKey::from_bip39_entropy(&entropy, passphrase.as_bytes());
        for index in &derivation.as_ref()[..3] {
            master = master.derive((*index).into());
        }
        let stake = master
            .derive(2)
            .derive(0)
            .to_public()
            .to_raw_key()
            .as_bytes();
        for index in &derivation.as_ref()[3..] {
            master = master.derive((*index).into());
        }
        let mut material = Zeroizing::new(master.as_bytes());
        material.extend_from_slice(&stake);
        return Ok((material, String::new()));
    }
    let seed = Zeroizing::new(mnemonic.to_seed(passphrase));
    if matches!(network, Network::Solana | Network::SolanaDevnet) {
        let mut mac = Hmac::<Sha512>::new_from_slice(b"ed25519 seed").map_err(invalid)?;
        mac.update(&*seed);
        let mut node = Zeroizing::new(mac.finalize().into_bytes().to_vec());
        for index in derivation.as_ref() {
            let mut mac = Hmac::<Sha512>::new_from_slice(&node[32..]).map_err(invalid)?;
            mac.update(&[0]);
            mac.update(&node[..32]);
            let index: u32 = (*index).into();
            mac.update(&index.to_be_bytes());
            node = Zeroizing::new(mac.finalize().into_bytes().to_vec());
        }
        return Ok((Zeroizing::new(node[..32].to_vec()), String::new()));
    }
    let secp = Secp256k1::new();
    let mut master = Xpriv::new_master(bitcoin::Network::Bitcoin, &*seed).map_err(invalid)?;
    let fingerprint = master.fingerprint(&secp).to_string();
    let mut child = master.derive_priv(&secp, &derivation).map_err(invalid)?;
    let result = Zeroizing::new(child.private_key.secret_bytes().to_vec());
    master.private_key.non_secure_erase();
    child.private_key.non_secure_erase();
    Ok((result, fingerprint))
}
pub fn public_account(secret: &[u8], network: Network, path: &str) -> Result<(String, String)> {
    if network.is_bitcoin() {
        return crate::wallet::public_account(&bytes(secret)?, network, path);
    }
    let public = if matches!(network, Network::Solana | Network::SolanaDevnet) {
        ed25519_dalek::SigningKey::from_bytes(&bytes(secret)?)
            .verifying_key()
            .to_bytes()
            .to_vec()
    } else if network == Network::Polkadot {
        schnorrkel::MiniSecretKey::from_bytes(secret)
            .map_err(invalid)?
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519)
            .public
            .to_bytes()
            .to_vec()
    } else if matches!(network, Network::Cardano | Network::CardanoPreprod) {
        if secret.len() != 128 {
            return Err(Error::Invalid("Cardano key material"));
        }
        let mut public = ada::Bip32PrivateKey::from_bytes(&secret[..96])
            .map_err(invalid)?
            .to_public()
            .to_raw_key()
            .as_bytes();
        public.extend_from_slice(&secret[96..]);
        public
    } else {
        let mut private = SecretKey::from_slice(secret).map_err(invalid)?;
        let public = private.public_key(&Secp256k1::new()).serialize().to_vec();
        private.non_secure_erase();
        public
    };
    Ok((address_from_public(&public, network)?, hex::encode(public)))
}
pub fn evm_address(bytes: &[u8]) -> Result<String> {
    let public = PublicKey::from_slice(bytes)
        .map_err(invalid)?
        .serialize_uncompressed();
    let hash = Keccak256::digest(&public[1..]);
    let lower = hex::encode(&hash[12..]);
    let checksum = hex::encode(Keccak256::digest(lower.as_bytes()));
    Ok(format!(
        "0x{}",
        lower
            .chars()
            .zip(checksum.bytes())
            .map(|(c, h)| if h >= b'8' { c.to_ascii_uppercase() } else { c })
            .collect::<String>()
    ))
}
pub fn ss58(public: &[u8]) -> Result<String> {
    let _: [u8; 32] = bytes(public)?;
    let mut data = vec![0];
    data.extend_from_slice(public);
    let mut h = blake2::Blake2b512::new();
    h.update(b"SS58PRE");
    h.update(&data);
    data.extend_from_slice(&h.finalize()[..2]);
    Ok(bs58::encode(data).into_string())
}
pub fn address_from_public(public: &[u8], network: Network) -> Result<String> {
    if network.evm_id().is_some() {
        return evm_address(public);
    }
    match network {
        Network::Solana | Network::SolanaDevnet => {
            ed25519_dalek::VerifyingKey::from_bytes(&bytes(public)?).map_err(invalid)?;
            Ok(bs58::encode(public).into_string())
        }
        Network::Polkadot => {
            schnorrkel::PublicKey::from_bytes(public).map_err(invalid)?;
            ss58(public)
        }
        Network::Cardano | Network::CardanoPreprod => {
            if public.len() != 64 {
                return Err(Error::Invalid("Cardano public keys"));
            }
            let payment = ada::PublicKey::from_bytes(&public[..32])
                .map_err(invalid)?
                .hash();
            let stake = ada::PublicKey::from_bytes(&public[32..])
                .map_err(invalid)?
                .hash();
            ada::BaseAddress::new(
                if network == Network::Cardano { 1 } else { 0 },
                &ada::Credential::from_keyhash(&payment),
                &ada::Credential::from_keyhash(&stake),
            )
            .to_address()
            .to_bech32(None)
            .map_err(invalid)
        }
        Network::BitcoinCash => {
            let key = TwPublic::new(public.to_vec(), PublicKeyType::Secp256k1).map_err(invalid)?;
            tw_bitcoincash::entry::BitcoinCashEntry
                .derive_address(&Context(network), key, Derivation::Default, None)
                .map(|a| a.to_string())
                .map_err(invalid)
        }
        Network::Tron | Network::TronNile => {
            let eth = super::hex_bytes(&evm_address(public)?)?;
            let mut payload = vec![0x41];
            payload.extend_from_slice(&eth);
            Ok(bs58::encode(payload).with_check().into_string())
        }
        Network::Cosmos | Network::Xrp | Network::Litecoin | Network::Dogecoin => {
            PublicKey::from_slice(public).map_err(invalid)?;
            let hash = hash160::Hash::hash(public).to_byte_array();
            if network == Network::Cosmos {
                return bech32::encode::<bech32::Bech32>(
                    bech32::Hrp::parse("cosmos").map_err(invalid)?,
                    &hash,
                )
                .map_err(invalid);
            }
            let prefix = match network {
                Network::Xrp => 0,
                Network::Litecoin => 48,
                _ => 30,
            };
            let mut data = vec![prefix];
            data.extend_from_slice(&hash);
            if network == Network::Xrp {
                Ok(bs58::encode(data)
                    .with_alphabet(bs58::Alphabet::RIPPLE)
                    .with_check()
                    .into_string())
            } else {
                Ok(bs58::encode(data).with_check().into_string())
            }
        }
        _ => Err(Error::Invalid("address network")),
    }
}
pub fn validate_public(account: &Account) -> Result<()> {
    validate_path(&account.path, account.network)?;
    let public = super::hex_bytes(&account.public_key)?;
    if address_from_public(&public, account.network)? != account.address {
        return Err(Error::Invalid("public key and address mismatch"));
    }
    Ok(())
}
pub fn evm_bytes(address: &str) -> Result<[u8; 20]> {
    if address.len() != 42 || !address.starts_with("0x") {
        return Err(Error::Invalid("EVM address length"));
    }
    let result = bytes(&super::hex_bytes(address)?)?;
    let body = &address[2..];
    if body.bytes().any(|b| b.is_ascii_uppercase()) && body.bytes().any(|b| b.is_ascii_lowercase())
    {
        let hash = hex::encode(Keccak256::digest(body.to_ascii_lowercase()));
        for (c, h) in body.bytes().zip(hash.bytes()) {
            if c.is_ascii_alphabetic() && (c.is_ascii_uppercase() != (h >= b'8')) {
                return Err(Error::Invalid("EVM address checksum"));
            }
        }
    }
    Ok(result)
}
pub fn address_bytes(address: &str, network: Network) -> Result<Vec<u8>> {
    if network.evm_id().is_some() {
        return Ok(evm_bytes(address)?.to_vec());
    }
    match network {
        Network::Solana | Network::SolanaDevnet => {
            let data = bs58::decode(address).into_vec().map_err(invalid)?;
            let _: [u8; 32] = bytes(&data)?;
            Ok(data)
        }
        Network::Polkadot => {
            let data = bs58::decode(address).into_vec().map_err(invalid)?;
            if data.len() != 35 || ss58(&data[1..33])? != address {
                return Err(Error::Invalid("Polkadot SS58 address"));
            }
            Ok(data[1..33].to_vec())
        }
        Network::Cardano | Network::CardanoPreprod => {
            let a = ada::Address::from_bech32(address).map_err(invalid)?;
            if a.network_id().map_err(invalid)? != if network == Network::Cardano { 1 } else { 0 } {
                return Err(Error::Invalid("Cardano network"));
            }
            Ok(a.to_bytes())
        }
        Network::Cosmos => {
            let (hrp, data) = bech32::decode(address).map_err(invalid)?;
            // The generic decoder accepts Bech32m too; Cosmos requires original Bech32.
            if hrp.as_str() != "cosmos"
                || data.len() != 20
                || bech32::encode::<bech32::Bech32>(hrp, &data).map_err(invalid)? != address
            {
                return Err(Error::Invalid("Cosmos address"));
            }
            Ok(data)
        }
        Network::Xrp => {
            let data = bs58::decode(address)
                .with_alphabet(bs58::Alphabet::RIPPLE)
                .with_check(Some(0))
                .into_vec()
                .map_err(invalid)?;
            if data.len() != 21 {
                return Err(Error::Invalid("XRP address"));
            }
            Ok(data[1..].to_vec())
        }
        Network::Tron | Network::TronNile => {
            let data = bs58::decode(address)
                .with_check(Some(0x41))
                .into_vec()
                .map_err(invalid)?;
            if data.len() != 21 {
                return Err(Error::Invalid("Tron address"));
            }
            Ok(data)
        }
        Network::Litecoin | Network::Dogecoin => {
            let prefix = if network == Network::Litecoin { 48 } else { 30 };
            let data = bs58::decode(address)
                .with_check(Some(prefix))
                .into_vec()
                .map_err(invalid)?;
            if data.len() != 21 {
                return Err(Error::Invalid("address"));
            }
            Ok(data[1..].to_vec())
        }
        Network::BitcoinCash => {
            let a = tw_bitcoincash::entry::BitcoinCashEntry
                .parse_address(&Context(network), address, None)
                .map_err(invalid)?;
            let legacy = match a {
                tw_bitcoincash::address::Address::Cash(cash) => {
                    cash.to_legacy(0, 5).map_err(invalid)?
                }
                tw_bitcoincash::address::Address::Legacy(legacy) => legacy,
            };
            let mut data = vec![legacy.prefix()];
            data.extend_from_slice(legacy.bytes());
            Ok(data)
        }
        _ => Err(Error::Invalid("address network")),
    }
}
