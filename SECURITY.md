# Security boundaries

AirVault is experimental wallet software. It has not had an independent security audit.

## Storage and trust boundaries

The native Rust process owns key derivation, secret encryption, generation, signing and verification. The webview performs UI work and transparent dice-to-word arithmetic; it does not derive seeds or sign. Newly generated phrases necessarily cross IPC once for their creation display. Ordinary snapshot/export commands do not return secrets.

A random 256-bit master key is wrapped with Argon2id (64 MiB, three iterations, one lane, a random 16-byte salt) and AES-256-GCM. Every record uses a random 256-bit key. Custom-password roots wrap that key only with the custom password; unlocking the outer vault does not decrypt those records. Passphrase records inherit their root's protection. Metadata, including encrypted secret records, is itself authenticated and encrypted. GCM nonces are independently generated 96-bit values. Associated data binds roles, version and identifiers. Master-password rotation rewraps the master key.

The entire authenticated file is written via a same-directory temporary file and atomic replacement. Backups contain encrypted secrets and require password reauthentication plus explicit UI confirmation. Restore never overwrites an existing profile. Unknown versions and unreasonable KDF/file bounds fail closed. This format is not yet a stable production interchange promise.

Native secret buffers use zeroization where supported. Rust crypto internals, webview strings, allocator copies, crash dumps, swap, hibernation and the OS may retain copies. This is not a defense against a compromised OS, malicious hardware, keyloggers or an attacker reading unlocked process memory. Windows ACLs are inherited from the selected profile directory; use a private local directory. Custom network locations need their own access-control review.

## Signing

Bitcoin signing uses P2WPKH single-key PSBT v0, SIGHASH_ALL. Every input requires its complete previous transaction; txid, vout, amount, script and witness UTXO must agree. Duplicate inputs, unknown/non-P2WPKH scripts, finalized/partially signed PSBTs and permissive sighash modes are rejected.

Review derives amounts, destinations and fees from transaction bytes. Descriptions are explicitly untrusted. Known addresses are labeled for the user to verify change, not automatically trusted as intent. An offline signer cannot independently know whether an output is unspent or whether a supplied blockchain history is current.

Deleting an account removes its public metadata and token list, preserves roots and identities, and invalidates outstanding signing approval. Pending requests and signed history remain; a deleted signing account must be rederived before dependent requests can be signed.

Each approval is bound to a request digest, expires after two minutes and is consumed on the first attempt, including failure. The vault password and any necessary custom root passwords are required. No-match and ambiguous-match inputs fail closed. Wallet Core and the documented native family libraries produce signatures. Separate verification checks exact transaction contents and native signatures using libsecp256k1, ed25519-dalek or schnorrkel as appropriate. See docs/engine-investigation.md. Imported responses are matched to their original request and reverified on review and broadcast.

Bitcoin limits reject fees over 1,000,000 satoshis or 500 sat/vB and transactions over 100 inputs/outputs. Exchange files are bounded to 8 MiB, decoded payloads to 4 MiB, and vault files to 32 MiB. These bounds accommodate authenticated Polkadot runtime metadata. Chain adapters also impose amount, input, field and lifetime bounds. A signed request cannot be signed again with the same request ID. Bitcoin's consensus rules remain the final authority on validity and double-spending.

Multi-chain requests admit typed operations only. EVM native/token/NFT calls use fixed ABI selectors; Solana messages use known programs; Cosmos messages use fixed type URLs. XRP and Tron fields are rebuilt from approved data. Litecoin/Dogecoin/BCH previous transactions are checked against their IDs and the signing account. Cardano inputs must be ADA-only and belong to the signing address. Polkadot Hub genesis, call argument types, signed extensions and the RFC78 metadata hash are bound into the operation.

Account token metadata must match incoming token requests before review. This prevents a request from substituting decimals or a familiar symbol for an unregistered contract, but does not authenticate the token issuer. Provider-derived balances, nonce, fee estimates, UTXO availability and block references remain external information.

## Modes, IPC and lifecycle

Proxy mode refuses secret import and signing, and a vault containing secret records cannot switch to Proxy. One application supports all modes. Offline mode checks precede network construction. No separate offline executable is distributed. A physical air gap requires physical/OS isolation; a webview runtime may have its own OS-managed behavior.

Windows suspend/resume callbacks request immediate invalidation before the next native command. Inactivity and optional focus loss also lock the session. UI polling clears visible secrets after native locking. The five-minute phrase ceremony permits correction after failed confirmation and is consumed only after a successful save. Cancellation, expiry, locking and leaving Create or import clear it. The UI also clears displayed words at the deadline. Explicitly confirmed seed copying sends a numbered phrase to the system clipboard; other apps and clipboard history may retain it, and AirVault does not claim to erase those external copies. Actual dice histories live only in component memory and never enter the vault, clipboard, diagnostics, exports or MCP.

The production application contains no test bridge, automation bypass or remote-debugging route. Test-only code resides under `tests/ui` and the `ui_test_bridge` Cargo example, which are separate entry points. There is no runtime switch to enable them in a release binary. Production artifact scans enforce their absence.

MCP is not implemented. No tool can unlock/sign/export secrets via an MCP interface. Implementing scoped, authenticated, disabled-by-default stdio MCP with human approval queues remains a later milestone.

## Dependency review

Trust Wallet Core 4.8.4 is pinned at `d40d24a63d92619167903369308bf0e2f7eb3a59`. Its internal Rust chain APIs are used because the upstream C++ desktop build does not officially support Windows. The integration must be retested when the pin changes.

At the initial test date, npm audit reported no vulnerabilities. Cargo audit reported no entries under known vulnerabilities, but did report nine unmaintained dependencies and two unsoundness warnings. The `atty` warning is in Wallet Core protobuf build tooling; `glib` is a Linux desktop dependency and absent from the Windows target graph. These warnings are not silently ignored and must be reviewed before a production release. This report is a point-in-time check, not a security guarantee.

## Multisig operations

Native SegWit Bitcoin multisig uses registered BIP48 sortedmulti policies. Keep the full public configuration plus independently stored seed/passphrase backups. A threshold of compromised keys can spend; too few surviving keys cannot be reset by AirVault. Configuration checksums are not authentication. Verify complete cosigner data and addresses independently before funding. Signed PSBTs can be combined and retained; completed transactions can be broadcast later. See [multisig risks and recovery](docs/multisig.md).

## Read-only protocol inspection

The payload inspector parses bounded untrusted bytes locally without signing or network access. Passing transport/envelope checksums proves only byte integrity. It does not authenticate transaction intent or execute scripts/contracts. The original-byte view remains available on decode errors, and unrecognized fields are explicit. Existing registered-wallet signing checks and independent response verification remain required for signing and broadcasting. See [verification boundaries](docs/payload-inspector.md#verification-boundaries).

## Public source and test data

Committed recovery phrases and deterministic entropy in test code are public test vectors. Test passwords, synthetic transactions and addresses are disposable fixtures, not user credentials or backups. Never use these keys for real funds. The Testnet4 probe keeps its generated random vault under the ignored .local directory.

Local profiles, wallet exchange files, signing artifacts, credentials, dependency caches and packaged binaries are excluded from Git. Only explicitly reviewed public fixtures belong in source control. This does not replace reviewing the staged files before publication.

Run Gitleaks against the intended source tree and reachable history before publishing. The checked-in .gitleaks.toml extends its default rules and permits only the exact public Cargo dependency name curve25519-dalek in Cargo.lock; it does not exclude the lockfile from scanning.

```bash
gitleaks git . --config .gitleaks.toml --redact
```
