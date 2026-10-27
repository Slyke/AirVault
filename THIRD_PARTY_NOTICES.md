# Third-party notices

The application uses open-source components under their respective licenses. This file is an inventory, not a replacement for upstream license texts.

- Trust Wallet Core 4.8.4: Apache-2.0, https://github.com/trustwallet/wallet-core/tree/4.8.4
- Tauri and plugins: MIT / Apache-2.0, https://github.com/tauri-apps/tauri
- Svelte: MIT, https://github.com/sveltejs/svelte
- rust-bitcoin and rust-secp256k1: CC0-1.0, https://github.com/rust-bitcoin/rust-bitcoin
- RustCrypto AES-GCM, Argon2, SHA-2 and related crates: MIT / Apache-2.0 (consult each locked package).
- BIP39 Rust crate: CC0-1.0, https://github.com/rust-bitcoin/rust-bip39
- BIP39 English wordlist: official Bitcoin BIPs repository, https://github.com/bitcoin/bips/blob/master/bip-0039/english.txt
  SHA-256 of bundled UTF-8 file: 2f5eed53a4727b4bf8880d8f3f199efc90e58503646d9ff8eff3a2ed3b24dbda.
- Microsoft WebView2Loader.dll: Microsoft WebView2 SDK redistributable, supplied by the locked webview2-com-sys dependency. Microsoft SDK terms apply: https://www.nuget.org/packages/Microsoft.Web.WebView2
  The bundled x64 loader has a valid Microsoft Corporation Authenticode signature.
- Cardano serialization library: MIT, https://github.com/Emurgo/cardano-serialization-lib
- schnorrkel / sr25519: BSD-3-Clause, https://github.com/w3f/schnorrkel
- ed25519-dalek and curve25519-dalek: BSD-3-Clause, https://github.com/dalek-cryptography/curve25519-dalek
- Substrate BIP39, SCALE and runtime metadata libraries: consult locked package license declarations, https://github.com/paritytech
- zxcvbn-ts core and bundled language dictionaries: MIT, https://github.com/zxcvbn-ts/zxcvbn
- Terminal, Paper and Ocean design tokens: copied from the user's adjacent styleguide repository.

Cargo.lock and package-lock.json identify the complete resolved dependency graph. A complete machine-generated license inventory and final redistribution review remain release tasks.

## Bitcoin whitepaper

The original English PDF, *Bitcoin: A Peer-to-Peer Electronic Cash System* by Satoshi Nakamoto (2008), is included unchanged for offline reference. Source: https://bitcoin.org/bitcoin.pdf (retrieved 2026-10-15). The bundled file and attribution are in public/reference; the portable package includes them in docs/reference. This historical document is not an audit or endorsement of AirVault.
