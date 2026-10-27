# AirVault

A Windows desktop vault for local keys, offline signing, and watch-only transfers. Built with Tauri, Svelte and Rust.

## Run on Windows

Extract **AirVault_0.1.0_windows-x64-portable.zip** and open **AirVault.exe**. Keep **WebView2Loader.dll** beside it. Alternatively use **AirVault_0.1.0_x64-setup.exe**. There is one application: it starts Offline and lets you choose Online or Proxy in Settings.

Windows 10/11 x64 and Microsoft Edge WebView2 Runtime are required. The installer does not download the runtime, so install WebView2 before moving to an offline machine. The application and installer are currently unsigned.

Create a vault password of at least six characters. The strength meter is advisory. Create or import a recovery phrase under **Create or import**, then choose a blockchain under Accounts. Add tokens to their blockchain account when creating it or later with **Include a token**. ERC-20 tokens can be added to Ethereum accounts.

Appearance includes D4rk, Terminal, Paper and Ocean, light/dark modes, fonts and 70–180% interface scale. Ctrl+/Ctrl− changes scale; Ctrl0 restores 100%. Basic and advanced instructions are on the Instructions page. Contextual explanations open through the small information buttons.

The default profile is in the Tauri application data directory for `app.airvault.desktop`. A separate profile can be selected with:

```powershell
.\AirVault.exe --data-dir C:\AirVault\signer
```

An existing profile is opened rather than overwritten. Restore is available on the creation screen of a fresh profile.

## Blockchains and assets

Bitcoin, Ethereum, Dogecoin, Solana, Polkadot Hub, Litecoin, Bitcoin Cash, BNB Smart Chain, Polygon, Avalanche C-Chain, Arbitrum, Optimism, Base, Cardano, Cosmos Hub, XRP Ledger and Tron have account derivation and native transfer signing. Additional networks include Bitcoin Testnet/Testnet4/Signet/Regtest, Ethereum Sepolia, Solana Devnet, Cardano Preprod and Tron Nile.

Token transfers cover ERC-20, SPL and TRC-20. The EVM transfer form also handles ERC-721 and ERC-1155 transfers. Cosmos supports delegation, undelegation and reward claims. See [capability details](docs/implementation-status.md) for exact transaction formats and remaining scope.

Online and Proxy modes need a provider URL for each network. No provider is contacted automatically. **Load network data** fills transaction counters, block references and available inputs. Review all transaction fields before signing or broadcasting.

For separate computers, export public accounts as `.avpub` to the Proxy profile. Prepare an `.avreq`, sign it on the Offline profile, and return the verified `.avsig` for explicit broadcast approval. Private keys remain on the signer.

## Development

Node 24, Rust, a C/C++ toolchain and Tauri prerequisites are required. Dependencies are locked; Wallet Core is pinned to commit `d40d24a63d92619167903369308bf0e2f7eb3a59`.

```bash
npm ci
npm run check
npm test
cargo test -p airvault-core --release
npm run test:ui
npm run tauri -- dev
```

From WSL, `bash scripts/build-windows.sh` builds one Windows application, the installer and portable ZIP. It requires the Windows GNU Rust target, MinGW and NSIS. See [build and test instructions](docs/testing.md).

The UI test harness is a separate Vite entry and native Cargo example, absent from the production application. The release scanner checks both assets and executable for test entry points.

See [security boundaries](SECURITY.md), [architecture](docs/architecture.md), [signing engines](docs/engine-investigation.md), and [validation](docs/validation.md). Windows is the current desktop target; Linux packaging and VM testing remain pending.

Wallet management is split into **Create or import**, **Wallets & recovery**, and **Passphrases**. Recovery entry accepts complete English words or unique four-letter prefixes, separated by spaces or lines. Numbered entries require one word per line in sequence. Failed import, verification and creation confirmation retain the input for correction. Creation confirmation can retry until its five-minute deadline. Copy seed phrase asks for confirmation and writes a numbered list to the clipboard.

Help buttons sit beside their headings and labels. Click outside a help dialog, press Escape or use Close to dismiss it. **Learn more** opens the relevant offline Instructions topic, with basic and advanced explanations. Accounts can be deleted with confirmation; the wallet, identities, history and blockchain funds remain intact.

## Bitcoin multisig

Open **Bitcoin multisig** for 2-of-3 or a custom threshold. Exchange public cosigner keys, verify the shared configuration, then use PSBT files in **Signing requests** to collect signatures. Keep each signer independent. Save the complete configuration for every cosigner and separate recovery phrases/passphrases; phrases alone are not a complete multisig backup. The offline wiki has Basic and Advanced setup, signing, recovery and risks articles. See [the multisig guide](docs/multisig.md).

## Payload inspection

The sidebar and signing reviews include a local protocol inspector with linked hex/ASCII highlighting, field explanations and integrity checks in all three operating modes. It accepts arbitrary files up to 8 MiB, hex, Base64 and AirVault envelopes. Bitcoin transaction/PSBT/P2P decoding is available alongside EVM transaction decoding. See [the inspection guide](docs/payload-inspector.md) for workflows and verification limits.

## Sparrow exchange

Bitcoin signing requests export standard signed PSBTs for single signature and multisig, plus complete raw transaction hex as `.txn`. Open either in Sparrow through **File → Open Transaction → File**. Unsigned P2WPKH PSBTs and unfinalized registered P2WSH multisig PSBTs can return to AirVault for signing. Previous transactions are required for every input. See [the Sparrow guide](docs/sparrow-interoperability.md) for steps, privacy/backup considerations and actual Windows round-trip evidence.
