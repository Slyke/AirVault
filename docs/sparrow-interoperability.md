# Sparrow interoperability

AirVault exports standard Bitcoin PSBT v0 files and fully signed raw transactions. These files can be inspected in Sparrow without importing a recovery phrase. The AirVault `.avreq` and `.avsig` envelopes remain useful for transfer between AirVault profiles; open the exported `.psbt` or `.txn` in Sparrow.

## Export from AirVault

1. Open **Signing requests** and review the inputs, destination, change and fee.
2. Sign with explicit approval. For multisig, collect the required signatures first.
3. Choose **Export signed PSBT** to retain previous transactions, key origins and signatures for another wallet. An incomplete multisig request instead offers **Export PSBT** for the next cosigner.
4. Choose **Export raw transaction** for a complete transaction. This writes lowercase transaction hex in a `.txn` text file, matching Sparrow's Save Final Transaction format.
5. In Sparrow, use **File → Open Transaction → File** (Ctrl+F) and select the file. Confirm the transaction ID, recipient, change and fee again. Opening or exporting a file does not broadcast it.

Both exports are available in Offline, Online and Proxy modes when the profile holds the required signed result. Proxy can export after importing the corresponding verified AirVault signature. Raw export is unavailable before the transaction is fully signed.

Use the PSBT for offline review of input values and fees. A raw transaction contains outputs and signatures but does not include the previous transactions or their input values; a wallet needs those separately to establish the fee. Key origins in a PSBT are public wallet metadata and reveal more information than the final transaction alone.

## Return a Sparrow PSBT to AirVault

For single signature, save the unsigned native SegWit P2WPKH PSBT from Sparrow, then use **Import PSBT** in AirVault. Select its network and review it before signing. AirVault exports a signed PSBT whose standard partial-signature fields let Sparrow finalize the transaction.

For multisig, register the same native SegWit P2WSH sortedmulti configuration in both applications. Use unfinalized PSBTs to exchange signatures. A Sparrow cosigner can sign first, AirVault can add its signature, and Sparrow can open the resulting PSBT and save its final transaction.

AirVault signing currently requires:

- PSBT version 0, native P2WPKH single signature or registered native P2WSH sortedmulti multisig, and SIGHASH_ALL.
- A complete previous transaction for every input, even when witness_utxo is present. Include this information when preparing the PSBT; an isolated signer cannot fetch it.
- Registered accounts for the input keys and the correct selected Bitcoin network.
- Unsigned single-signature inputs. Multisig inputs may contain valid partial signatures but must remain unfinalized.

Single-signature PSBTs already signed by another application and finalized PSBTs can be inspected in the payload inspector; they are not signing-round imports. Raw transactions are also inspection/export artifacts rather than imports into the signing queue.

AirVault preserves an existing request's original transaction semantics when it sees the same unsigned transaction again. Importing a signing PSBT invalidates an outstanding approval, so review and approve the current request again.

## Backup and review

PSBTs are transaction working files, not wallet backups. Keep recovery phrases and BIP39 passphrases, plus the complete multisig configuration and all cosigner key origins, threshold and descriptors. Keep independent cosigners on separate devices for a real multisig wallet. A public descriptor or master public key cannot replace a cosigner's private recovery material.

Anyone holding a fully signed transaction may broadcast it. PSBTs and transaction files can disclose addresses, amounts and wallet relationships; transfer them deliberately. A matching transaction ID does not by itself verify witness signatures because SegWit witnesses are outside the txid commitment. AirVault independently verifies complete signatures before raw export.

## Tested with Sparrow 2.5.5 on Windows, 2026-10-24

The actual Sparrow desktop application ran with an isolated Offline regtest profile and publicly known test-vector keys. No user wallet was opened.

- **Single signature:** Sparrow opened AirVault's signed PSBT, recognized its signature and saved the final transaction. AirVault verified the returned transaction and compared its bytes with its own raw export: identical. Sparrow also saved an unsigned PSBT that AirVault imported and signed successfully.
- **2-of-3 multisig:** Sparrow signed with one software cosigner, saved a one-signature PSBT, and AirVault added a second cosigner signature. Sparrow recognized both signatures, finalized the returned PSBT and saved the transaction. AirVault independently verified the returned transaction; both raw exports were byte-identical.
- **Raw files:** Sparrow directly opened AirVault's single-signature and multisig `.txn` exports with the expected transaction IDs and complete signatures.
- **Independent chain check:** Bitcoin Core 31.1 finalized the single-signature PSBT to the same raw bytes. Isolated regtest tests accepted and mined both transaction families.
- **Native regression checks:** signed exports survive lock/unlock, work after verified signature import on Proxy, refuse incomplete raw transactions and reject corrupted signed responses.

The official standalone Windows ZIP had SHA-256 `da62b27effb93e83cc8fba4705248a03ec6683577c32aa6bc32801dc3b89b6e7`. Its signed manifest was verified with fingerprint `D4D0D3202FC06849A257B38DE94618334C674B40`. These checks qualify the tested formats above; they do not cover every wallet, script type or hardware device.

## Reproduce the desktop test

Use disposable profiles and public test fixtures only. Never fund these keys on mainnet.

1. Run the isolated Bitcoin Core regtest integration with `AIRVAULT_SPARROW_FIXTURES=/absolute/disposable/directory` to save funded unsigned PSBTs.
2. Run `cargo run -p airvault-core --example sparrow_roundtrip -- prepare /absolute/disposable/directory`. Without the Core fixtures, this creates structurally valid transactions with synthetic, unspendable funding instead.
3. Start a separate Sparrow profile with `Sparrow.exe -d C:\\temporary\\sparrow-profile -n regtest`; keep it Offline. Open the single signed PSBT and save its final transaction. Save a copy of the unsigned PSBT for the AirVault return test.
4. Import `multi-watch-descriptor.txt` as a Sparrow Output Descriptor wallet. For the test cosigning round only, replace keystore 2 using Software Wallet → Master Private Key (BIP32), with the public-vector key from `multi-public-fixture-master.txt` and derivation `m/48'/1'/0'/2'`. Verify the master fingerprint `397a4a50` and matching account key in `fixture-info.json`. Descriptor import alone creates a watch-only wallet in the tested Sparrow version.
5. Open `multi-unsigned.psbt`, finalize it for signing, sign with that one Sparrow cosigner, and save an unfinalized partial PSBT.
6. Run `sparrow_roundtrip sign-multi <directory> <partial.psbt> <output-prefix>` to add AirVault's local cosigner signature. Open that output PSBT in Sparrow and save its final transaction.
7. Run `sparrow_roundtrip verify-single|verify-multi <directory> <sparrow-final.txn>` and compare the returned hex file byte-for-byte with the AirVault raw export. The `sign-single` command similarly exercises unsigned PSBT reimport.
8. Close an existing tab for the same transaction before opening a different file variant: Sparrow may reuse the transaction tab. Directly open each `.txn` too.

The harness is a Cargo example absent from the production application. It uses published test-vector entropy and imports multiple fixture roots to construct its test setup; this does not model key separation. The independent-profile native multisig tests cover separate signer vaults.

See [Sparrow downloads](https://sparrowwallet.com/download/), [Sparrow 2.5.5 release](https://github.com/sparrowwallet/sparrow/releases/tag/2.5.5), and [BIP174 PSBT specification](https://github.com/bitcoin/bips/blob/master/bip-0174.mediawiki).
