# Architecture

## Components

`src/` contains the Svelte UI. `src-tauri/` owns narrow IPC commands, native file paths, profile exclusivity and Windows power-event notifications. `crates/airvault-core/` holds encrypted storage, identities, entropy, native transaction handling, approval sessions and explicit network adapters.

The native command layer serializes mutations with one session mutex. Network calls copy only approved public data and run outside the mutex, allowing timeout/suspend locking while a provider is pending. Completed broadcast receipts are queued in memory when locked and added to encrypted history on unlock. No generic shell/SQL/executable dispatch exists. Browser content security policy restricts connections to Tauri IPC. No third-party scripts, web fonts, analytics or network logger are loaded.

## File protocol

Version 1 uses strict camelCase JSON envelopes with unknown-field rejection.

- `.avpub`: public accounts with network, address, public key, path, label, fingerprint and token records. No local identity reference. Imported addresses must match their public keys and derivation purpose.
- `.avreq`: unique request ID, operation, explicit network, base64 PSBT or typed native transaction, SHA-256 payload checksum and untrusted description.
- `.avsig`: original request ID and full-envelope digest, network, base64 signed transaction, its checksum and txid.
- `.avvault`: versioned password-wrapped master key and encrypted authenticated metadata/records.

Payload hashes detect corruption; they do not authenticate sender intent. The signer treats the whole request as untrusted, decodes its native payload and binds local approval to the full typed envelope digest. The proxy verifies every signature against the exact unsigned request before accepting a response.

Full request serialization is the v1 binding format; consumers should parse/serialize the documented schema rather than rely on arbitrary JSON field order. Transfer medium is independent of AirVault.

## UI secret lifecycle

Creation uses a native, five-minute, single-use ceremony. The UI receives the phrase only when it generates it. Hiding removes that display, and confirmation compares the entered phrase to the native held value before encrypting. Confirmation, cancellation, navigation and lock clear actual dice worksheets. Recovery verification compares an entered candidate without redisplaying a stored phrase.

Dice-to-word conversion is integer arithmetic against the official bundled English list. It is deliberately inspectable. The native generator separately maps prefix words to their exact 11-bit entropy indices, fills the remainder from the OS, and uses the established BIP39 library for checksums. Tests reconstruct checksum bits independently with SHA-256.

## Styleguide adaptations

Terminal, Paper and Ocean surface/radius tokens come from `../styleguide/design.md`; D4rk remains the default. Root `data-palette`, `data-theme` and `data-font` drive the UI. Only these non-secret preferences and interface scale are stored in browser localStorage using the guide's key names. Monospace is used throughout, as appropriate for the initial wallet interface. Semantic start/mid/warning/danger meanings remain intact.

The guide's server/Docker probes and HTTP MCP preference are not applicable to this native offline-first product. The full generic logging infrastructure was intentionally not copied because it adds unneeded network sinks. Startup records only build/version identifiers; operation parameters, entropy, keys, phrases and passphrases are never logged. The product specification takes precedence for local stdio MCP, when implemented.

## Bitcoin multisig

Optional account policies preserve backward-compatible single-key public-bundle serialization. They contain a BIP48 cosigner set, threshold and explicit branch/index. A local account references one proven identity; watch-only export clears the identity. .avmulti backups include public configuration and checksummed receive/change descriptors. PSBT v0 .psbt files carry unfinalized signing rounds.

The original request remains immutable. Encrypted vault progress stores the combined PSBT separately. Imports match unsigned transaction bytes and selected network, validate all partial signatures and registered scripts, and merge only verified signatures. An import invalidates outstanding approvals. Once every input reaches its threshold, finalization creates a normal .avsig response bound to the local original request; broadcast still independently verifies its final witnesses. New requests and failed rounds use the existing transactional mutation/rollback boundary.

## Payload inspector

A separate native inspector decodes bounded, user-selected bytes without network access, signing primitives or a vault reference. Reports contain immutable byte documents, contiguous field ranges, explanatory metadata and separately scoped checks. Binary/hex/Base64 normalization retains the exact source document and source SHA-256; envelope and PSBT children get independent offset spaces. The frontend pages bytes and links hover/focus/pin selection across hex, ASCII and named fields. File/text commands run parsing in a blocking worker; exchange inspection copies only the selected public transaction data from the session. Both build feature paths register the same read-only commands. Native import/signing/broadcast validation remains independent of the explanatory decoder.
