#![cfg(feature = "network")]
// Explicit, read-only interoperability check. This module is never linked into the desktop app.
use airvault_core::{
    model::{Mode, Network, Settings, VaultData},
    providers,
    storage::OpenVault,
};

#[test]
#[ignore = "contacts public chain providers; run explicitly"]
fn public_provider_handshakes() {
    let mut vault = OpenVault::create("123456", VaultData::default()).unwrap();
    let identity=vault.import_phrase("Public fixture","abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",None).unwrap();
    for (network, url) in [
        (
            Network::Polkadot,
            "wss://polkadot-asset-hub-rpc.polkadot.io",
        ),
        (Network::SolanaDevnet, "https://api.devnet.solana.com"),
    ] {
        let id = vault
            .derive_account(
                identity,
                network.chain().name,
                network,
                network.chain().path,
                None,
            )
            .unwrap();
        let account = vault.data.accounts.iter().find(|a| a.id == id).unwrap();
        let mut settings = Settings::default();
        settings.mode = Mode::Combined;
        settings.providers.insert(
            serde_json::to_value(network)
                .unwrap()
                .as_str()
                .unwrap()
                .into(),
            url.into(),
        );
        let state = providers::fetch_state(&settings, account)
            .unwrap_or_else(|e| panic!("{}: {e}", network.chain().name));
        assert!(state.is_object());
        if network == Network::Polkadot {
            assert!(state["metadata"].as_str().unwrap().len() > 1000);
        } else {
            assert_eq!(
                bs58::decode(state["recentBlockhash"].as_str().unwrap())
                    .into_vec()
                    .unwrap()
                    .len(),
                32
            );
        }
        println!(
            "{}: verified chain identity and loaded native transaction state",
            network.chain().name
        );
    }
}
