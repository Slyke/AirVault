# Validation record

## Signed PSBT and raw transaction exports, 2026-10-24

- Full native suite: 41 passing tests; the separate live-provider test remains opt-in. Network-disabled core compilation passes. The four export/multisig integration tests also pass as Windows executables.
- Bitcoin Core 31.1: both isolated regtest tests pass. Core finalizes the signed single-signature PSBT to identical raw bytes; single-signature and multisig transactions are accepted and mined. No external peers are enabled.
- Actual Sparrow 2.5.5 Windows desktop: single-signature signed PSBT open/finalization/export and unsigned PSBT save/reimport/sign pass. A Sparrow software cosigner signs a 2-of-3 PSBT, AirVault adds the second signature, and Sparrow recognizes and finalizes both. Returned single-signature and multisig raw bytes match AirVault exactly and pass independent verification. Both AirVault raw .txn exports also open directly in Sparrow.
- Sparrow ran Offline with an isolated temporary profile and public known test vectors. No existing user wallet was opened. The temporary Sparrow instance was closed and its window exit confirmed. Official ZIP checksum and signed-manifest verification are recorded in the interoperability guide.
- Svelte/TypeScript: zero errors or warnings. JavaScript: 13 passing tests. All 25 browser flows pass, including actual-native signed PSBT/raw export, wiki links, inspector and multisig flows.
- The final extracted Windows package was exercised through computer-use MCP with a disposable signed public-fixture profile. Both native Save As exports wrote byte-identical PSBT and raw transaction files compared with the independently tested core outputs. The test instance was closed.
- Windows EXE, NSIS installer and portable ZIP were rebuilt; the package includes the Sparrow guide. Independent source fingerprint, executable hash, archive byte comparison and SHA256SUMS checks pass. The production frontend/executable scan finds no test or automation hooks.
- See [tested scope, exact steps and fixture harness](sparrow-interoperability.md).

## Payload inspector, 2026-10-24

- Full native unit/integration suite: 40 passing tests. Live-provider and Bitcoin Core regtest tests remain opt-in and were not rerun for this read-only inspector.
- Eight new inspector scenarios pass on Linux and as Windows GNU native executables. Coverage includes exact byte-range coverage, binary/hex/Base64 detection, txid/wtxid, corrupt/truncated/non-canonical transactions, duplicate inputs, PSBT key duplication, UTXO/script commitments and amount bounds, envelope hash failures, concatenated P2P checksums, block merkle failures, canonical EVM legacy/type-1/type-2 RLP, ABI word segmentation and arbitrary hostile bytes.
- The new corruption tests include independently computed SHA-256/double-SHA256 values rather than trusting the explanatory decoder's output.
- Svelte/TypeScript: zero errors or warnings. All 13 JavaScript tests pass. All 25 browser flows pass against the separate native Rust bridge, including four inspector flows, paired hex/ASCII hover and keyboard navigation, pinning, arbitrary-extension files, checksum failure display, current PSBT review, unchanged transaction/vault data, all three operating modes and linked Basic/Advanced instructions.
- Network-disabled core compilation also passes. Native inspector commands are registered in both desktop feature paths, and the module has no network or vault dependency.
- Computer-use MCP exercised the extracted Windows portable executable in a fresh disposable profile: create vault in Offline mode, open the standalone inspector, native All Files picker, load an arbitrary .dat Bitcoin P2P transaction message, verify Passed checksum/native-parser results, and inspect linked hex/ASCII highlighting plus a pinned previous-txid explanation. The test application was closed and its process exit verified. No existing user vault was opened.
- The Windows executable, NSIS installer and portable ZIP were rebuilt. The package includes the inspection guide. Independent source fingerprint, binary SHA-256, archive byte identity and SHA256SUMS checks pass; production frontend and executable scans find no test bridge or automation hooks.
- Unsupported formats and unsupported PSBT versions remain inspectable as raw bytes. Scope limits are explicit: transport/file integrity is separate from signature execution, spend validity, contract execution and chain acceptance.


## Bitcoin multisig, 2026-10-15

- Added BIP48 native SegWit P2WSH sortedmulti setup, explicit cosigner registration, public configuration backups with BIP380 receive/change descriptors, and standard PSBT v0 signing rounds.
- Full native core suite: 32 passing unit/integration tests. Three additional multisig scenarios cover separate profiles, parallel signature merging, per-input 3-of-3 thresholds, invalid signatures, incomplete final witnesses, wrong networks, approval invalidation, duplicate cosigners, wrong identity ownership, persistence, corrupted configuration and phrase restoration.
- Svelte/TypeScript: zero errors or warnings; all 13 JavaScript tests pass. All 21 browser flows pass against the actual Rust bridge, including configuration review/backup/load, next-index preparation, partial signing, PSBT export and all four new wiki topics at both reading levels.
- Bitcoin Core 31.1 regtest, with no external peers, independently derives matching receive/change addresses, verifies descriptor checksums and PSBT output origins, refuses completion with one signature, finalizes the same two-signature PSBT into identical transaction bytes, accepts it into the mempool and confirms it in a mined block.
- The three multisig tests also pass as Windows GNU native executables using their own writable disposable temporary vaults.
- Computer-use MCP exercised the packaged Windows executable with an isolated, publicly known fixture profile: unlock, multisig setup layout, contextual help and correct Learn more destination, stored partial-signature review showing 1/2, and the waiting-for-another-cosigner state. No real vault or funded wallet was used. The test application exited successfully and was closed.
- Final Windows portable package and installer include the new feature. The portable package includes docs/multisig.md. Independent source hashing, binary hashing, archive byte comparison and SHA256SUMS verification pass. The executable and frontend pass the production test-hook scan.
- This tests native SegWit Bitcoin multisig only. Regtest confirmation does not assert compatibility with every hardware wallet. Imported external PSBTs must meet the documented full-previous-transaction and unfinalized PSBT-v0 restrictions. No Linux desktop packaging or VM test was performed.


## Wiki, locking and application identity, 2026-10-15

- Svelte/TypeScript: zero errors or warnings; 13 JavaScript tests pass.
- Native core: 29 passing unit/integration tests. The new checks cover the -1 timeout sentinel, invalid intervals, persistence, manual/focus/stall/restart locking, runtime file hashing, and source identity changes for edited/new inputs. Live-provider and regtest checks remain opt-in and were not repeated for this UI/settings change.
- Browser UI: all 20 flows pass against the native Rust bridge. Coverage includes all 23 wiki articles at both reading levels, search/related/back navigation, all mode explanations and the Proxy restriction, contextual entropy/locking links, -1 save/lock/unlock persistence, real executable hash comparison, byte-identical offline PDF export and spacing around dice alternatives/review. Initial selector and async-wait failures were corrected in the tests.
- Windows-native core unit executable: all 12 tests pass, including never-inactivity locking and current-executable hashing.
- Computer-use MCP checked the packaged Windows app in a fresh disposable profile: About before unlock; source/Git/build identity; runtime EXE SHA-256 matching independent PowerShell Get-FileHash; successful vault creation; wiki layout/search/reference navigation; and the native Save As dialog for the bundled paper. The save dialog was dismissed without an export during that smoke check; automated export and packaged PDF checks passed independently.
- The runtime EXE digest matches BUILD-INFO.json. Portable packaging occurs after NSIS resource finalization so both distribution paths use the final executable bytes. Source metadata and packaged reference/checksums are independently checked.
- Production assets and executable pass the test-hook scan. The disposable desktop instance was closed. No existing user wallet was opened or changed.


## UX revision, 2026-10-14

- Svelte/TypeScript: zero errors or warnings. JavaScript: 13 passing tests.
- Native core: 26 passing unit/integration tests; live-provider and Bitcoin regtest checks remain opt-in. New coverage checks every English BIP39 word's four-letter expansion, numbered-line validation, correction after failed confirmation, and deletion without loss of wallet recovery.
- Browser UI: all 14 flows pass against the real Rust test bridge. New flows exercise clipboard confirmation/cancellation and numbered output, checksum retries without clearing input, rejected one-line numbering, import/recovery using four-letter words, separate wallet/passphrase pages, help alignment/backdrop/Escape behavior, direct instruction-topic navigation, both signing-request import buttons, and account deletion/cancellation/rederivation across lock/unlock.
- Three initial new UI failures were test-selector issues (the exact entropy label and simultaneous progress/result status elements). The corrected tests all pass.
- The Windows-native unit executable passes all 10 tests, including the new parser, retry and account-deletion checks.
- Computer-use MCP verified the packaged Windows app opens, creates a disposable vault in Offline mode, shows the separate wallet pages, places root/passphrase help beside its label/title, dismisses help by clicking outside, follows Learn more to the correct Instructions topic, and displays both signing-request import actions. This visual check caught and prompted correction of an inherited vertical layout in the topic menu.
- Browser tests use disposable profiles and public fixtures. No user's existing wallet was opened or modified. The disposable desktop test instance was closed afterward.

## Previous chain and Windows validation


Updated Windows implementation, 2026-10-13.

- TypeScript/Svelte: zero errors or warnings.
- JavaScript: 13 passing tests, including all 2,048 BIP39 indices, wordlist SHA-256, entropy controls, predictable-password grading and scale bounds.
- Rust: 23 passing core/integration tests. The multi-chain test covers derivation, approval, signing, independent verification, tamper rejection and signer/proxy file exchange for all 20 additional network entries.
- Independent bip_utils 2.12.2 address vectors cover every newly added key/address family, including Cardano Icarus and Polkadot sr25519 hard derivation.
- Token tests cover ERC-20, SPL and TRC-20; EVM NFT tests cover ERC-721/ERC-1155. Cosmos tests cover delegate, undelegate and reward claims. Mislabelled token decimals and substituted transaction semantics are rejected.
- Local provider fixtures cover native and token balance parsing, transaction-state loading, chain identity rejection, secure URL rules and Offline mode. Completed broadcast receipts survive a session lock and are saved on unlock.
- Browser UI: eight passing end-to-end flows against actual native encryption and signing. These cover compact/no-scroll setup at 1280×940, the six-character minimum without strength enforcement, confirmation matching, hidden restore for existing vaults, default Offline mode, appearance/theme persistence, Ctrl scale controls, dice ceremony clearing, all chain selectors, and ERC-20 account creation/addition/approved signing.
- Polkadot signatures are tested against v15 metadata retrieved from the current Polkadot Hub runtime (spec 2005000). Metadata hashing and signed-extension validation are enabled.
- Dependency checks: npm audit reports zero vulnerabilities. Cargo audit reports zero vulnerability entries, nine unmaintained warnings and two unsoundness warnings. The atty unsoundness warning concerns build tooling; glib is outside the Windows target graph.

The Bitcoin Core 31.1 regtest round trip was rerun successfully: accepted by testmempoolaccept, broadcast and mined. A disposable Testnet4 wallet was also queried successfully; it remains unfunded. New chain fixtures establish signing/encoding behavior, not funded live-network settlement.

The Windows x64 release, NSIS installer and portable ZIP were rebuilt successfully. All 23 core tests passed when run directly as Windows executables. An additional native Windows live-provider test verified Polkadot Hub WSS/runtime metadata and Solana Devnet HTTPS/blockhash loading without broadcasting.

The packaged application was extracted to local Windows storage and exercised through the computer-use MCP: compact 1280×940 content area without a default scrollbar; Ctrl+/Ctrl− synchronized the scale control; a six-character password graded Very weak was accepted; confirmation displayed only matching; the resulting vault opened Offline. The disposable test instance was closed after verification.

Launching through the automated host from a WSL UNC executable path stalled before process creation; running the locally extracted ZIP succeeded. Extract the portable package to a local Windows folder for testing. Production scans require test bridge/automation hooks to be absent from assets and executable. The portable ZIP contains one application executable plus its required WebView2Loader.dll. The loader is Microsoft-signed; AirVault and its installer remain unsigned.

Manual sleep/resume, installer upgrade/uninstall, live funded public-network transfers, independent security audit, and Linux desktop VM testing remain separate checks.
