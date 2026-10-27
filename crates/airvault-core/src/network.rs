//! All network entry points require an explicit mode and provider.
use crate::{
    model::{Mode, Network},
    protocol::MAX_EXCHANGE_BYTES,
    Error, Result,
};
use serde::Deserialize;
use std::{io::Read, time::Duration};

pub(crate) fn endpoint(mode: Mode, base: &str, path: &str) -> Result<String> {
    if !mode.can_network() {
        return Err(Error::Mode);
    }
    let url = url::Url::parse(base).map_err(|_| Error::Invalid("provider URL"))?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if (url.scheme() != "https" && !(loopback && url.scheme() == "http"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Invalid("provider must use HTTPS (HTTP is allowed only for loopback), without credentials, query or fragment"));
    }
    Ok(format!("{}/{}", base.trim_end_matches('/'), path))
}
pub(crate) fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(20))
        .build()
}
pub(crate) fn body(response: ureq::Response) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    response
        .into_reader()
        .take(MAX_EXCHANGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Network)?;
    if bytes.len() > MAX_EXCHANGE_BYTES {
        return Err(Error::Invalid("provider response size"));
    }
    Ok(bytes)
}
pub(crate) fn get(mode: Mode, base: &str, path: &str) -> Result<Vec<u8>> {
    let url = endpoint(mode, base, path)?;
    body(agent().get(&url).call().map_err(|_| Error::Network)?)
}
pub fn verify_chain(mode: Mode, base: &str, network: Network) -> Result<()> {
    if !network.is_bitcoin() {
        return Err(Error::Invalid("Bitcoin provider network"));
    }
    let hash = String::from_utf8(get(mode, base, "block-height/0")?).map_err(|_| Error::Network)?;
    let expected = ::bitcoin::blockdata::constants::genesis_block(network.native())
        .block_hash()
        .to_string();
    if hash.trim() != expected {
        return Err(Error::Invalid(
            "provider genesis block does not match the selected network",
        ));
    }
    Ok(())
}
#[derive(Deserialize)]
struct RemoteUtxo {
    txid: String,
    vout: u32,
}
pub fn utxos(
    mode: Mode,
    base: &str,
    address: &str,
    network: Network,
) -> Result<Vec<crate::bitcoin::Utxo>> {
    address
        .parse::<::bitcoin::Address<_>>()
        .map_err(|_| Error::Invalid("address"))?
        .require_network(network.native())
        .map_err(|_| Error::Invalid("address network"))?;
    verify_chain(mode, base, network)?;
    let remote: Vec<RemoteUtxo> =
        serde_json::from_slice(&get(mode, base, &format!("address/{address}/utxo"))?)?;
    if remote.len() > 100 {
        return Err(Error::Invalid("more than 100 UTXOs; use coin control"));
    }
    let mut result = vec![];
    for item in remote {
        item.txid
            .parse::<::bitcoin::Txid>()
            .map_err(|_| Error::Network)?;
        let raw = get(mode, base, &format!("tx/{}/hex", item.txid))?;
        let transaction_hex = String::from_utf8(raw)
            .map_err(|_| Error::Network)?
            .trim()
            .to_owned();
        let transaction: ::bitcoin::Transaction = ::bitcoin::consensus::deserialize(
            &hex::decode(&transaction_hex).map_err(|_| Error::Network)?,
        )
        .map_err(|_| Error::Network)?;
        if transaction.compute_txid().to_string() != item.txid {
            return Err(Error::Verification);
        }
        let output = transaction
            .output
            .get(item.vout as usize)
            .ok_or(Error::Verification)?;
        let expected = address
            .parse::<::bitcoin::Address<_>>()
            .map_err(|_| Error::Network)?
            .require_network(network.native())
            .map_err(|_| Error::Network)?;
        if output.script_pubkey != expected.script_pubkey() {
            return Err(Error::Verification);
        }
        result.push(crate::bitcoin::Utxo {
            transaction_hex,
            vout: item.vout,
        });
    }
    Ok(result)
}
pub fn address_summary(
    mode: Mode,
    base: &str,
    address: &str,
    network: Network,
) -> Result<serde_json::Value> {
    address
        .parse::<::bitcoin::Address<_>>()
        .map_err(|_| Error::Invalid("address"))?
        .require_network(network.native())
        .map_err(|_| Error::Invalid("address network"))?;
    verify_chain(mode, base, network)?;
    let value = serde_json::from_slice(&get(mode, base, &format!("address/{address}"))?)?;
    Ok(value)
}
pub fn broadcast(mode: Mode, base: &str, raw: &[u8], network: Network) -> Result<String> {
    verify_chain(mode, base, network)?;
    let url = endpoint(mode, base, "tx")?;
    let response = agent()
        .post(&url)
        .set("Content-Type", "text/plain")
        .send_string(&hex::encode(raw))
        .map_err(|_| Error::Network)?;
    let txid = String::from_utf8(body(response)?)
        .map_err(|_| Error::Network)?
        .trim()
        .to_owned();
    txid.parse::<::bitcoin::Txid>()
        .map_err(|_| Error::Network)?;
    Ok(txid)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_never_constructs_a_network_request() {
        assert!(matches!(
            endpoint(Mode::Offline, "https://example.org", "tx"),
            Err(Error::Mode)
        ));
    }
    #[test]
    fn rejects_unsafe_providers() {
        for url in [
            "http://example.org",
            "https://user:pass@example.org",
            "file:///tmp/x",
            "https://example.org?token=x",
        ] {
            assert!(endpoint(Mode::Proxy, url, "tx").is_err());
        }
        assert!(endpoint(Mode::Combined, "http://127.0.0.1:3000", "tx").is_ok());
    }
}
