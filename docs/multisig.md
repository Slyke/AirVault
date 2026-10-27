# Bitcoin multisig

AirVault supports native SegWit P2WSH sortedmulti wallets on Bitcoin, Testnet, Testnet4, Signet and Regtest. The default is 2-of-3; configurations accept 2–15 independent cosigners and thresholds from 1 to N. Other blockchains are outside this multisig feature.

## Set up

1. Generate and verify a separate recovery phrase on each independent signer. Back up any BIP39 passphrase separately.
2. Open **Bitcoin multisig**, select the network and local identity, and show its public cosigner key.
3. Exchange only the public entries, formatted as `[fingerprint/48h/coinh/accounth/2h]xpub` (or tpub). Use the same complete set on every device.
4. Choose the threshold, receive/change branch and address index. Review all complete keys and origins, threshold, network and address through a trusted channel.
5. Register on each signer with its own local identity. Register on a coordinator with **Watch only**.
6. Save the public configuration on each device. Rehearse recovery and a small spend before funding substantially.

Separate profiles on one computer are test conveniences, not independent security boundaries. Multisig creates new addresses; existing single-key funds must be transferred to them.

## Signing rounds

Prepare the transaction on the connected coordinator, export a PSBT, and import it under **Signing requests → Multisig PSBT exchange** on an offline signer. Select the network explicitly. Review every output, amount and fee, approve with the vault password, and export the updated PSBT. Repeat on enough independent signers, or import independently signed copies into the coordinator. AirVault validates and combines signatures by unsigned transaction and selected network. Every input must satisfy its own threshold before a complete response becomes available.

Return the PSBT to the connected coordinator, review again and explicitly approve broadcast. A PSBT does not carry a reliable network identifier. The original .avreq does not include later signatures; .avsig carries only a complete response bound to the original AirVault request ID. PSBT exchange lets devices with different local request IDs converge.

Imports accept binary or base64 PSBT v0 files with .psbt extension, unfinalized P2WSH inputs, full previous transactions, witness scripts and SIGHASH_ALL. Export from other software before finalization and include full previous transactions. Legacy/nested multisig, Taproot, mixed single-key/multisig transactions and arbitrary scripts are not accepted.

## Back up

Keep **all** of the following:

- The complete **public configuration for every cosigner**, not just the keys that usually sign: threshold, network, script type, all xpubs, fingerprints, origin paths, receive/change descriptors.
- Each signer's independently stored recovery phrase and exact BIP39 passphrase, if used. Recovering requires at least M usable private cosigners.
- Used receive/change branches and indices, highest used indices, a known receive address, and useful transaction IDs/creation dates for recovery scanning.
- Optional encrypted .avvault backups, with separately recoverable vault and custom-root passwords.

**Save configuration backup** writes .avmulti with the complete public policy, current branch/index, receive/change ranged descriptors with BIP380 checksums, and a consistency checksum. These checksums detect corruption, not malicious substitution. Authenticate the initial configuration and compare a previously recorded address after restoring. Public configuration cannot spend by itself, but exposes financial privacy.

To rehearse, restore independent signers into fresh offline profiles from their phrases and passphrases, load the configuration, select the correct identity, register the used indices and compare addresses. Complete a small test-network transaction using the restored threshold. AirVault registers individual indices; it does not automatically scan descriptor ranges. Compatible descriptor wallets can reconstruct and scan the family from the exported descriptors.

## Risks and replacement

For M-of-N, M compromised keys can spend and more than N−M unavailable keys can prevent spending permanently. 1-of-N does not resist one stolen key; N-of-N cannot tolerate one missing signer. Keep threshold secrets away from a common device, location or cloud account, and plan for unavailable people and inheritance.

A malicious coordinator can propose an unwanted transaction. Verify destination, amount, fee and configuration independently on each signer. A signature is an approval that can be retained; a completed transaction may be broadcast later while its inputs remain unspent.

Labels, deletion and reconfiguration do not revoke a key. To replace a compromised or lost signer, use the surviving threshold to move funds to a newly verified wallet. There is no administrator reset when too few keys survive. Multisig spends also have larger witnesses and generally cost more than single-key SegWit spends.

## Technical boundaries and evidence

BIP48 account origins are `m/48'/coin'/account'/2'`; child branch 0 receives, branch 1 changes. Each child key set is sorted per BIP67. Local ownership compares a complete derived account xpub, not the short fingerprint. Imported PSBT metadata never supplies private derivation instructions.

Rust-bitcoin assembles PSBTs and scripts; libsecp256k1 produces ECDSA signatures. Every partial and final witness is verified before accepting, merging or broadcasting. Full previous transactions bind input values. Approval is single-use and invalidated by PSBT imports. Vault mutation is transactional: failure preserves the prior signing round.

Tests cover independent signers, parallel merging, per-input thresholds, altered signatures, incomplete witnesses, incorrect networks, duplicate cosigners, wrong local ownership, configuration corruption, encrypted persistence and seed restoration. An isolated Bitcoin Core regtest independently compares descriptors and addresses, finalizes the same PSBT, rejects completion with one signature, and accepts and confirms the two-signature spend.

Standards: [BIP48](https://github.com/bitcoin/bips/blob/master/bip-0048.mediawiki), [BIP67](https://github.com/bitcoin/bips/blob/master/bip-0067.mediawiki), [BIP174](https://github.com/bitcoin/bips/blob/master/bip-0174.mediawiki), [BIP380](https://github.com/bitcoin/bips/blob/master/bip-0380.mediawiki). The offline wiki includes separate Basic and Advanced articles for setup, signing, backups/recovery and risks.
