# Implementation status

## Current Windows application

One application defaults to Offline. Online and watch-only Proxy are settings on the same executable. The opening screen is compact, passwords require six characters with advisory grading, confirmation reports matching, and persistent appearance scale is synchronized with Ctrl+/Ctrl−/Ctrl0.

| Family | Derivation and signing |
| --- | --- |
| Bitcoin and its test networks | BIP44/49/84/86 address derivation; P2WPKH single-key and BIP48 native SegWit P2WSH sortedmulti PSBT v0 signing with SIGHASH_ALL |
| Ethereum, Sepolia, BNB Smart Chain, Polygon, Avalanche C, Arbitrum, Optimism, Base | BIP44 secp256k1; EIP-155 legacy native transfers; ERC-20; ERC-721 and ERC-1155 transfers |
| Litecoin, Dogecoin, Bitcoin Cash | BIP44 P2PKH spending; complete previous transactions; BCH fork-ID signatures and CashAddr |
| Solana and Devnet | Hardened SLIP-10 Ed25519; SOL and SPL TransferChecked to existing associated token accounts |
| Polkadot Hub | Substrate BIP39 sr25519 root and numeric hard paths; Balances.transfer_keep_alive; v15 runtime metadata bound using CheckMetadataHash |
| Cardano and Preprod | Icarus/CIP-1852 extended Ed25519 base addresses; ADA-only UTXOs and native witnesses |
| Cosmos Hub | BIP44 secp256k1; protobuf Send, Delegate, Undelegate and ClaimRewards |
| XRP Ledger | BIP44 secp256k1; XRP Payment with optional destination tag and last-ledger bound |
| Tron and Nile | BIP44 secp256k1; TRX and TRC-20 protobuf transfers with reference block and expiration |

Token records belong to accounts. An incoming token request must match the signer's registered contract/mint, symbol and decimals. ERC-20 tokens are not separate networks. Token identity still requires the user to verify the contract; a label alone is not provenance.

Providers cover native balances, EVM/SPL/TRC-20 token balances, transaction state, and explicit broadcast. APIs are Esplora, Blockbook v2, EVM/Solana/XRP JSON-RPC, Polkadot Hub WSS, Cosmos REST, Tron FullNode HTTP and Cardano Koios v1. Provider fixtures test the parsing and mode boundaries; they are not a claim that every hosted provider or every transaction has been tested on a live network.

Storage includes independent mnemonic roots, BIP39 passphrase identities, Bitcoin compressed WIF import, per-root password wrapping, vault password rotation, backup and fresh-profile restore. Requests require a short-lived single-use review and password authentication. Signed bytes are reconstructed and verified independently of the signing engine.

## Remaining specification scope

Linux desktop packaging awaits the user's VM. The following broader items are not implemented: Bitcoin legacy/nested/Taproot multisig and non-P2WPKH single-key signing; automatic account discovery and descriptor range scanning; general contract calls and typed-message signing; Solana Token-2022/automatic associated-account creation; Cardano native assets; XRP issued assets; Tron TRC-10; staking outside Cosmos; general NFT inventory; confirmation tracking and automatic fee estimation; polished coin selection; root/account editing and per-key policies; custom-root password rotation UI; and the scoped MCP server.

Polkadot targets Hub, where DOT asset operations run, and signs version-4 immortal extrinsics with nonce and runtime replay protection. Additional Substrate chains and soft/non-numeric derivation paths are not included. Cardano uses Icarus derivation, not Ledger seed derivation. Numeric amounts and EVM token IDs are bounded to 128 bits.

Unsigned Windows artifacts are available for testing. Independent security audit, release signing and reproducibility qualification remain release work. See SECURITY.md and validation.md for the current evidence and limits.

## Wallet usability

Create/import, current wallets/recovery checks, and BIP39 passphrases have separate navigation pages. Phrase import and confirmation normalize full words or unique four-letter prefixes; numbered phrases require sequential one-word-per-line input. Failed checks preserve editable input, and creation confirmation can retry within five minutes. Seed copying requires confirmation and emits numbered lines. Account deletion requires confirmation and preserves roots, identities, requests and history. Contextual help is aligned with headings/labels; offline Instructions offers topic-based basic and advanced explanations.

## Bitcoin multisig

Configurable native SegWit M-of-N setup, public cosigner exchange, confirmed registration, configuration backups with receive/change descriptors, verified PSBT signing rounds and finalization are available. Accounts track explicit branch/index entries. See [setup, backup, risks and exact interoperability boundaries](multisig.md).

## Payload inspector

All three operating modes provide a standalone arbitrary-file inspector and signing-review integration. Bitcoin raw transactions, PSBTv0 and unencrypted P2P v1 messages have field decoding and integrity checks; EVM legacy/type-1/type-2 transactions have RLP and call-data segmentation. AirVault envelopes check schema and payload hashes, and expose decoded payloads. Other chain requests expose typed JSON. Unsupported data remains readable as raw bytes, with verification scope stated explicitly. See [payload-inspector.md](payload-inspector.md).

## Sparrow interoperability

Standard PSBT v0 exports include P2WPKH single-signature signatures and existing P2WSH multisig signing rounds. Complete signed Bitcoin requests export verified raw transaction hex as .txn. Both single-signature and 2-of-3 multisig exchanges were exercised with actual Sparrow 2.5.5 on Windows, including mixed Sparrow/AirVault cosigning and byte-identical final transaction exports. Unsigned single-signature PSBT import uses the same restrictive review/signing adapter. See [the guide and test evidence](sparrow-interoperability.md).
