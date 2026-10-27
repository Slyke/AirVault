//! Explicit, bounded provider calls. Only public account data and approved signed bytes cross this boundary.
mod state;
use crate::{
    chains::{self, invalid, keys},
    model::{Account, Mode, Network, Settings},
    network,
    protocol::{Response, MAX_EXCHANGE_BYTES},
    Error, Result,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
pub use state::fetch_state;
use std::{
    net::{TcpStream, ToSocketAddrs},
    time::{Duration, Instant},
};

fn get(mode: Mode, base: &str, path: &str) -> Result<Value> {
    serde_json::from_slice(&network::get(mode, base, path)?).map_err(Into::into)
}
fn post(mode: Mode, base: &str, path: &str, value: Value) -> Result<Value> {
    let url = network::endpoint(mode, base, path)?;
    let bytes = network::body(
        network::agent()
            .post(&url)
            .set("Content-Type", "application/json")
            .send_string(&value.to_string())
            .map_err(|_| Error::Network)?,
    )?;
    serde_json::from_slice(&bytes).map_err(Into::into)
}
fn rpc(mode: Mode, base: &str, method: &str, params: Value) -> Result<Value> {
    let request = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
    let response = if base.starts_with("wss://") || base.starts_with("ws://") {
        websocket(mode, base, request)?
    } else {
        post(mode, base, "", request)?
    };
    if response.get("error").is_some_and(|e| !e.is_null()) {
        return Err(Error::Network);
    }
    response.get("result").cloned().ok_or(Error::Network)
}
fn websocket(mode: Mode, base: &str, request: Value) -> Result<Value> {
    if !mode.can_network() {
        return Err(Error::Mode);
    }
    let url = url::Url::parse(base).map_err(invalid)?;
    let host = url.host_str().ok_or(Error::Invalid("provider host"))?;
    let local = matches!(host, "localhost" | "127.0.0.1" | "[::1]");
    if !(url.scheme() == "wss" || (local && url.scheme() == "ws"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Invalid(
            "use a secure WebSocket provider without credentials or query parameters",
        ));
    }
    let port = url
        .port_or_known_default()
        .ok_or(Error::Invalid("provider port"))?;
    let mut stream = None;
    for address in (host, port)
        .to_socket_addrs()
        .map_err(|_| Error::Network)?
        .take(4)
    {
        if let Ok(s) = TcpStream::connect_timeout(&address, Duration::from_secs(5)) {
            stream = Some(s);
            break;
        }
    }
    let stream = stream.ok_or(Error::Network)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .map_err(|_| Error::Network)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(20)))
        .map_err(|_| Error::Network)?;
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(MAX_EXCHANGE_BYTES))
        .max_frame_size(Some(MAX_EXCHANGE_BYTES));
    let (mut socket, _) = tungstenite::client_tls_with_config(base, stream, Some(config), None)
        .map_err(|_| Error::Network)?;
    socket
        .send(tungstenite::Message::Text(request.to_string().into()))
        .map_err(|_| Error::Network)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        match socket.read().map_err(|_| Error::Network)? {
            tungstenite::Message::Text(text) => {
                let data: Value = serde_json::from_str(&text)?;
                if data.get("id") == Some(&json!(1)) {
                    let _ = socket.close(None);
                    return Ok(data);
                }
            }
            tungstenite::Message::Ping(_) => {
                socket.flush().map_err(|_| Error::Network)?;
            }
            tungstenite::Message::Close(_) => break,
            _ => {}
        }
    }
    Err(Error::Network)
}
fn text(value: &Value) -> Result<&str> {
    value.as_str().ok_or(Error::Network)
}
fn integer(value: &Value) -> Result<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .ok_or(Error::Network)
}
fn hex_number(value: &Value) -> Result<u128> {
    u128::from_str_radix(text(value)?.strip_prefix("0x").ok_or(Error::Network)?, 16)
        .map_err(invalid)
}
fn provider(settings: &Settings, network: Network) -> Result<String> {
    if !settings.mode.can_network() {
        return Err(Error::Mode);
    }
    let url = settings.provider(network);
    if url.is_empty() {
        return Err(Error::Invalid(
            "Set a provider for this network in Settings",
        ));
    }
    Ok(url)
}
pub fn verify_chain(mode: Mode, base: &str, chain: Network) -> Result<()> {
    if chain.is_bitcoin() {
        return network::verify_chain(mode, base, chain);
    }
    let valid = if let Some(id) = chain.evm_id() {
        hex_number(&rpc(mode, base, "eth_chainId", json!([]))?)? == id as u128
    } else {
        match chain {
            Network::Solana | Network::SolanaDevnet => {
                text(&rpc(mode, base, "getGenesisHash", json!([]))?)?
                    == if chain == Network::Solana {
                        "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"
                    } else {
                        "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"
                    }
            }
            Network::Polkadot => {
                text(&rpc(mode, base, "chain_getBlockHash", json!([0]))?)?.trim_start_matches("0x")
                    == chains::transactions::polkadot::GENESIS
            }
            Network::Cosmos => {
                get(mode, base, "cosmos/base/tendermint/v1beta1/node_info")?["default_node_info"]
                    ["network"]
                    == "cosmoshub-4"
            }
            Network::Xrp => {
                integer(&rpc(mode, base, "server_info", json!([{}]))?["info"]["network_id"])? == 0
            }
            Network::Tron | Network::TronNile => {
                text(&post(mode, base, "wallet/getblockbynum", json!({"num":0}))?["blockID"])?
                    .ends_with(if chain == Network::Tron {
                        "2b6653dc"
                    } else {
                        "cd8690dc"
                    })
            }
            Network::Cardano | Network::CardanoPreprod => {
                integer(&get(mode, base, "genesis")?[0]["networkmagic"])?
                    == if chain == Network::Cardano {
                        764824073
                    } else {
                        1
                    }
            }
            Network::Dogecoin | Network::Litecoin | Network::BitcoinCash => {
                let data = get(mode, base, "")?;
                let expected = match chain {
                    Network::Dogecoin => "Dogecoin",
                    Network::Litecoin => "Litecoin",
                    _ => "Bitcoin Cash",
                };
                data["blockbook"]["coin"] == expected && data["backend"]["chain"] == "main"
            }
            _ => false,
        }
    };
    if !valid {
        return Err(Error::Invalid(
            "provider does not match the selected blockchain",
        ));
    }
    Ok(())
}
pub fn balance(settings: &Settings, account: &Account) -> Result<Value> {
    let base = provider(settings, account.network)?;
    let mode = settings.mode;
    let n = account.network;
    let address = &account.address;
    verify_chain(mode, &base, n)?;
    if n.is_bitcoin() {
        let data = network::address_summary(mode, &base, address, n)?;
        let confirmed = &data["chain_stats"];
        let pending = &data["mempool_stats"];
        let funded = integer(&confirmed["funded_txo_sum"])?
            .checked_add(integer(&pending["funded_txo_sum"])?)
            .ok_or(Error::Network)?;
        let spent = integer(&confirmed["spent_txo_sum"])?
            .checked_add(integer(&pending["spent_txo_sum"])?)
            .ok_or(Error::Network)?;
        let amount = funded.checked_sub(spent).ok_or(Error::Network)?;
        return Ok(
            json!({"network":n.chain().name,"address":address,"balance":chains::display_units(amount as u128,8),"symbol":"BTC","assets":[]}),
        );
    }
    let value = if n.evm_id().is_some() {
        hex_number(&rpc(
            mode,
            &base,
            "eth_getBalance",
            json!([address, "latest"]),
        )?)?
    } else {
        match n {
            Network::Solana | Network::SolanaDevnet => integer(
                &rpc(
                    mode,
                    &base,
                    "getBalance",
                    json!([address,{"commitment":"confirmed"}]),
                )?["value"],
            )? as u128,
            Network::Cosmos => {
                let response = get(
                    mode,
                    &base,
                    &format!("cosmos/bank/v1beta1/balances/{address}"),
                )?;
                let amounts = response["balances"].as_array().ok_or(Error::Network)?;
                amounts
                    .iter()
                    .find(|a| a["denom"] == "uatom")
                    .map(|a| text(&a["amount"]).and_then(chains::units))
                    .transpose()?
                    .unwrap_or(0)
            }
            Network::Xrp => chains::units(text(
                &rpc(
                    mode,
                    &base,
                    "account_info",
                    json!([{"account":address,"ledger_index":"validated"}]),
                )?["account_data"]["Balance"],
            )?)?,
            Network::Tron | Network::TronNile => post(
                mode,
                &base,
                "wallet/getaccount",
                json!({"address":address,"visible":true}),
            )?["balance"]
                .as_u64()
                .unwrap_or(0) as u128,
            Network::Cardano | Network::CardanoPreprod => chains::units(text(
                &post(mode, &base, "address_info", json!({"_addresses":[address]}))?[0]["balance"],
            )?)?,
            Network::Polkadot => state::polkadot_balance(mode, &base, address)?,
            Network::Litecoin | Network::Dogecoin | Network::BitcoinCash => chains::units(text(
                &get(mode, &base, &format!("address/{address}"))?["balance"],
            )?)?,
            _ => return Err(Error::Invalid("balance network")),
        }
    };
    let mut assets = vec![];
    for token in &account.tokens {
        token.validate(n)?;
        let amount = if n.evm_id().is_some() {
            let data = format!(
                "0x70a08231000000000000000000000000{}",
                hex::encode(keys::evm_bytes(address)?)
            );
            hex_number(&rpc(
                mode,
                &base,
                "eth_call",
                json!([{"to":token.contract,"data":data},"latest"]),
            )?)?
        } else if matches!(n, Network::Solana | Network::SolanaDevnet) {
            let ata = chains::transactions::solana::associated(address, &token.contract, n)?;
            let info = rpc(
                mode,
                &base,
                "getAccountInfo",
                json!([ata,{"encoding":"jsonParsed","commitment":"confirmed"}]),
            )?;
            if info["value"].is_null() {
                0
            } else {
                let data = &info["value"]["data"]["parsed"]["info"];
                if data["mint"] != token.contract
                    || integer(&data["tokenAmount"]["decimals"])? != token.decimals as u64
                {
                    return Err(Error::Invalid(
                        "token mint or decimals disagree with provider",
                    ));
                }
                chains::units(text(&data["tokenAmount"]["amount"])?)?
            }
        } else if matches!(n, Network::Tron | Network::TronNile) {
            let parameter = format!(
                "000000000000000000000000{}",
                hex::encode(&keys::address_bytes(address, n)?[1..])
            );
            let result = post(
                mode,
                &base,
                "wallet/triggerconstantcontract",
                json!({"owner_address":address,"contract_address":token.contract,"function_selector":"balanceOf(address)","parameter":parameter,"visible":true}),
            )?;
            if result["result"]["result"] != true {
                return Err(Error::Network);
            }
            u128::from_str_radix(text(&result["constant_result"][0])?, 16).map_err(invalid)?
        } else {
            continue;
        };
        assets.push(json!({"symbol":token.symbol,"contract":token.contract,"balance":chains::display_units(amount,token.decimals)}));
    }
    Ok(
        json!({"network":n.chain().name,"address":address,"balance":chains::display_units(value,n.chain().decimals),"symbol":n.chain().symbol,"assets":assets}),
    )
}
pub fn broadcast(settings: &Settings, response: &Response) -> Result<String> {
    let n = response.network;
    let mode = settings.mode;
    let base = provider(settings, n)?;
    verify_chain(mode, &base, n)?;
    let raw = response.payload.decode()?;
    if n.is_bitcoin() {
        return network::broadcast(mode, &base, &raw, n);
    }
    let txid = if n.evm_id().is_some() {
        text(&rpc(
            mode,
            &base,
            "eth_sendRawTransaction",
            json!([format!("0x{}", hex::encode(&raw))]),
        )?)?
        .to_owned()
    } else {
        match n {
        Network::Solana|Network::SolanaDevnet=>text(&rpc(mode,&base,"sendTransaction",json!([STANDARD.encode(&raw),{"encoding":"base64","skipPreflight":false,"preflightCommitment":"confirmed"}]))?)?.to_owned(),
        Network::Polkadot=>text(&rpc(mode,&base,"author_submitExtrinsic",json!([format!("0x{}",hex::encode(&raw))]))?)?.to_owned(),
        Network::Cosmos=>{let result=post(mode,&base,"cosmos/tx/v1beta1/txs",json!({"tx_bytes":STANDARD.encode(&raw),"mode":"BROADCAST_MODE_SYNC"}))?;
            if integer(&result["tx_response"]["code"])?!=0{return Err(Error::Network);}text(&result["tx_response"]["txhash"])?.to_owned()},
        Network::Xrp=>{let result=rpc(mode,&base,"submit",json!([{"tx_blob":hex::encode_upper(&raw)}]))?;
            if !["tesSUCCESS","terQUEUED"].contains(&text(&result["engine_result"])?) {return Err(Error::Network);}text(&result["tx_json"]["hash"])?.to_owned()},
        Network::Tron|Network::TronNile=>{let result=post(mode,&base,"wallet/broadcasthex",json!({"transaction":hex::encode(&raw)}))?;
            if result["result"]!=true{return Err(Error::Network);}result["txid"].as_str().unwrap_or(&response.txid).to_owned()},
        Network::Cardano|Network::CardanoPreprod=>{
            let url=network::endpoint(mode,&base,"submittx")?;
            let bytes=network::body(network::agent().post(&url).set("Content-Type","application/cbor").send_bytes(&raw).map_err(|_|Error::Network)?)?;
            text(&serde_json::from_slice::<Value>(&bytes)?)?.to_owned()
        },
        Network::Dogecoin|Network::Litecoin|Network::BitcoinCash=>{
            let url=network::endpoint(mode,&base,"sendtx/")?;
            let bytes=network::body(network::agent().post(&url).set("Content-Type","text/plain").send_string(&hex::encode(&raw)).map_err(|_|Error::Network)?)?;
            text(&serde_json::from_slice::<Value>(&bytes)?["result"])?.to_owned()
        },
        _=>return Err(Error::Invalid("broadcast network")),
    }
    };
    if txid != response.txid {
        return Err(Error::Verification);
    }
    Ok(txid)
}
