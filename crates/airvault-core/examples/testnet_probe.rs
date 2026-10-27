//! Disposable Testnet4 profile and read-only provider interoperability check.
#[cfg(feature = "network")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use airvault_core::{
        model::{Mode, Network},
        network,
        session::Session,
    };
    use zeroize::Zeroize;
    let directory = std::path::PathBuf::from(".local/testnet4");
    std::fs::create_dir_all(&directory)?;
    let mut session = Session::new(directory.join("vault.avvault"));
    let password = "airvault-testnet-only";
    if session.snapshot().initialized {
        session.unlock(password)?;
    } else {
        session.create(password, Mode::Combined)?;
        let mut ceremony = session.generate(24, "", None)?;
        let identity = session.confirm_phrase(
            ceremony.token,
            &ceremony.phrase,
            "Disposable Testnet4 wallet",
            None,
        )?;
        ceremony.phrase.zeroize();
        session.mutate(|v| {
            v.derive_account(
                identity,
                "Testnet4 receive",
                Network::Testnet4,
                "m/84'/1'/0'/0/0",
                None,
            )?;
            Ok(())
        })?;
        let mut settings = session.snapshot().settings.unwrap();
        settings.rpc_url = "https://mempool.space/testnet4/api".into();
        session.settings(settings, password, true)?;
    }
    let snapshot = session.snapshot();
    let account = &snapshot.accounts[0];
    let url = &snapshot.settings.unwrap().rpc_url;
    let summary = network::address_summary(Mode::Combined, url, &account.address, account.network)?;
    println!(
        "{}",
        serde_json::json!({"network":"testnet4","address":account.address,"provider":url,"summary":summary})
    );
    session.lock();
    Ok(())
}
#[cfg(not(feature = "network"))]
fn main() {
    panic!("This test utility requires the network feature");
}
