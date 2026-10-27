# Windows testing and development

## Basic manual test

1. Extract the entire portable ZIP. Keep **WebView2Loader.dll beside AirVault.exe**. Copying only an EXE is insufficient for this Windows GNU build. Alternatively, use the installer, which includes the loader.
2. Start AirVault with a disposable profile. Create a vault (Offline is the default) and open Create or import.
3. Name the wallet. The default is 24 words with 11 manually generated prefix words and 135 OS-generated bits. For a short UI test, set the prefix count to 1. Use actual fair rolls for any real entropy; deterministic test values are only for testing.
4. Enter eleven die values. Check immediate bits, undo/reset, the resulting word, expandable plain-text calculation, printed position and verification checkbox.
5. Generate the phrase. Record it privately, hide it, re-enter it, and confirm. The complete phrase and its dice history should no longer appear.
6. Derive a BIP84 Testnet4 account. Test startup locking and recovery verification. Never fund the published all-abandon test vector.
7. In Appearance, try Terminal, Paper and Ocean, dark/light modes, fonts, scale and Ctrl+/Ctrl−/Ctrl0 shortcuts. Close/reopen to check preference persistence.
8. Create an encrypted backup and use a new profile to check and then restore it. Confirm the derived address is unchanged.
9. Use the Bitcoin Core regtest integration below for repeatable funded signing. The live Testnet4 probe only checks provider/network/address interoperability unless the disposable address is funded.

## Signing in two profiles

Use an Offline signer profile and a Proxy profile. Export `.avpub` from the signer and import it on the proxy. Configure an Esplora endpoint for the account's network; `https://mempool.space/testnet4/api` was checked during development. Provider state is external and cannot prove offline spendability.

Prepare a request using UTXOs including full previous transaction hex. Export `.avreq`, import it on the signer, inspect every input/output/fee, enter the required passwords and select the explicit approval checkbox. Export `.avsig` and import it into the proxy. Review its independently verified contents before choosing broadcast. Wrong passwords and failed approvals consume the approval token; close and reopen review before trying again.

## Automated checks

```bash
npm ci
npm run check
npm test
cargo test -p airvault-core --release
cargo test -p airvault-core --no-default-features
npm run test:ui
npm run build
npm run check:production
```

`npm run test:ui` builds a separate Rust example and starts a loopback Vite test server. The test entry mocks only the Tauri transport: actual create/unlock, encryption, generation, confirmation, account derivation, review and signing call the real core. Test profiles use temporary directories. The bridge never ships. Browser traces/screenshots are disabled by default to avoid retaining generated phrases.

The original eight UI cases cover compact setup, advisory password grading, confirmation matching, scale shortcuts, all blockchain account selectors, ERC-20 registration/signing, theme/mode persistence and offline help, dice undo/reset and prefix preservation, full 128-roll generation/checksum, clearing worksheets, and real signing with an invalid password followed by explicit valid approval. Unit cases check all 2,048 dice indices and the official wordlist hash. Rust checks independently reconstruct entropy/checksum bits with SHA-256, outside the BIP39 parser.

Core integration checks cover encrypted storage, password rotation, custom-password isolation, tampering, BIP44/49/84/86 vectors, Unicode passphrases, malicious PSBTs, request/response binding, signature verification, single-use approvals and backup restoration. The Windows test executables can be cross-built and run directly on Windows:

```bash
cargo test -p airvault-core --release --target x86_64-pc-windows-gnu --no-run
```

## Bitcoin Core regtest

Obtain Bitcoin Core from its official release site and verify its checksum/signatures as appropriate. This workspace used Bitcoin Core 31.1. The test starts an isolated node with no external peers, funds a disposable address, matures the coinbase, transfers files between signer/proxy sessions, signs with Wallet Core, runs Bitcoin Core's mempool acceptance check, broadcasts, mines a block and confirms inclusion.

```bash
BITCOIND="$PWD/.tools/bitcoin/bitcoin-31.1/bin/bitcoind" \
cargo test -p airvault-core --test regtest -- --ignored --nocapture
```

The download's SHA-256 was compared with the official published checksum. GPG release-signature verification was not performed in this session. Do not describe that as a full supply-chain verification.

The separate `cargo run -p airvault-core --example testnet_probe` creates `.local/testnet4/vault.avvault` with the disposable password `airvault-testnet-only`, generates a fresh random root, and queries Testnet4 without printing the phrase. It does not broadcast or request faucet funds. This profile is for testing only and must never hold mainnet funds.

## Build Windows

The maintained cross-build entry is `bash scripts/build-windows.sh`. Prerequisites are Rust's `x86_64-pc-windows-gnu` target, MinGW GCC, Node 24 and NSIS. The local build cache uses NSIS under `.tools/nsis/root/usr`; the wrapper restores NSISDIR because Tauri clears that variable. NSIS 3.08 needed the upstream `Include/Win/RestartManager.nsh` file for the current Tauri template. A current NSIS release is preferable.

The build produces the single application, NSIS installer, loader DLL, portable ZIP and hashes. Tauri automatically adds the GNU WebView2 loader to the installer. The portable packager explicitly copies and requires it. WebView2 Runtime itself must already be installed; no runtime download is triggered.

For native Windows MSVC builds, install Rust, Visual Studio C++ tools, Node and Tauri's Windows prerequisites, then use `npm run tauri -- build`. This host was validated using the cross-built GNU binaries; native MSVC build qualification remains separate.

## Known test limits

Native Windows launch and vault creation are checked via the computer-use MCP tool. Core test binaries run on Windows. Real sleep/resume, focus-loss behavior with native file dialogs, installer upgrade/uninstall, and a funded public Testnet4 broadcast still need dedicated manual coverage. Linux desktop packaging/VM UI testing remains pending.

## Recovery and help regressions

Use public all-abandon fixtures for clipboard and error-path testing. Verify numbered one-word-per-line copying requires confirmation; Cancel leaves the clipboard untouched. Test malformed checksum, numbered words on a single line, full words, four-letter prefixes and newline entry. Errors must retain the entry and allow retry without consuming a creation ceremony. Confirm that successful verification, navigation, expiry and locking clear the relevant secret state.

Verify the three wallet pages are separate, all page titles have help, field help stays beside its label, help dialogs close on backdrop/Escape, and Learn more selects the correct Instructions topic. Account deletion must require confirmation, persist across lock/unlock, leave the wallet and identities intact, and allow rederiving the same address.

## Bitcoin multisig checks

Run `cargo test -p airvault-core --release --test multisig` for independent-signer and recovery cases. Run `BITCOIND=/absolute/path/to/verified/bitcoind cargo test -p airvault-core --release --test regtest multisig -- --ignored --nocapture` for Bitcoin Core descriptor/PSBT interoperability and confirmed 2-of-3 spending. The node disables external peers. The native bridge browser flow is `npm run test:ui -- multisig.spec.ts`. Windows cross-compiled multisig test executables require writable disposable temporary directories.

## Payload inspector

Run the airvault-core inspector integration tests for byte-range coverage, independent transaction identifiers, PSBT corruption and UTXO/script commitments, concatenated P2P messages/checksums, block merkle commitments, canonical EVM RLP and hostile/truncated input handling. The tests/ui/inspector.spec.ts browser tests exercise paired hex/ASCII hover and keyboard behavior, pinning, arbitrary-extension files, corrupt checksums, review source selection and all three modes. The isolated bridge provides public fixtures only and is absent from production.

## Sparrow exports and round trips

Run `cargo test -p airvault-core --test exports --test multisig` for signed PSBT/raw export, Proxy reconstruction, persistence, incomplete-transaction gating and corruption refusal. The real-signing browser flow tests both export buttons against the native core. The complete browser suite also checks the Sparrow instructions article and its related/contextual links.

For the desktop exchange, use [the Sparrow interoperability guide](sparrow-interoperability.md). The `sparrow_roundtrip` example builds public-vector disposable fixtures, signs PSBTs saved by Sparrow and independently verifies transactions returned by Sparrow. The optional `AIRVAULT_SPARROW_FIXTURES` directory on the Core regtest integration exports funded fixtures for this harness. Neither helper is part of the production executable.
