# Signing engines

Trust Wallet Core 4.8.4 is pinned to commit `d40d24a63d92619167903369308bf0e2f7eb3a59`. Its upstream C++ build does not officially support Windows, so AirVault uses its native Rust chain modules.

| Family | Signing engine | Verification |
| --- | --- | --- |
| BTC/LTC/DOGE | Wallet Core BitcoinEntry | rust-bitcoin consensus, scripts and libsecp256k1 |
| BCH | Wallet Core BitcoinCashEntry | reconstructed fork-ID digest and libsecp256k1 |
| EVM | Wallet Core EthereumEntry | canonical RLP, EIP-155 chain binding, Keccak and libsecp256k1 recovery |
| Solana | Wallet Core SolanaEntry | exact legacy message reconstruction and ed25519-dalek |
| Cosmos | Wallet Core CosmosEntry | independent SignDoc/TxRaw protobuf encoding and libsecp256k1 |
| XRP | Wallet Core RippleEntry | exact native field serialization, SHA-512Half and DER signature verification |
| Tron | Wallet Core secp256k1 signer | native protobuf reconstruction, SHA-256 and libsecp256k1 recovery |
| Cardano | cardano-serialization-lib 15 | exact body/witness checks, Blake2b256 and ed25519-dalek |
| Polkadot Hub | schnorrkel sr25519 | Substrate signing context, runtime call/extension types, RFC78 metadata digest |

BIP39, rust-bitcoin BIP32, SLIP-10 HMAC, substrate-bip39 and Cardano Icarus provide the family-specific derivations. All default address families are compared against independently generated bip_utils 2.12.2 public vectors. Cardano and common Substrate sr25519 derivation require their respective native libraries; no incompatible Wallet Core ed25519 fallback is used for Polkadot.

Runtime metadata is decoded and validated before constructing a Polkadot call. Unknown signed extensions fail closed. CheckMetadataHash is enabled and its digest is signed. Native operations are typed; arbitrary calldata, arbitrary Solana messages and arbitrary executable calls do not enter a generic signing endpoint.

References:

- [Wallet Core source](https://github.com/trustwallet/wallet-core/tree/4.8.4)
- [Wallet Core platform FAQ](https://developer.trustwallet.com/developer/wallet-core/faq)
- [Cardano serialization library](https://github.com/Emurgo/cardano-serialization-lib)
- [Substrate metadata RFC78](https://polkadot-fellows.github.io/RFCs/approved/0078-merkleized-metadata.html)
- [Blockbook API](https://github.com/trezor/blockbook/blob/master/openapi.yaml)
- [Tron constant contract queries](https://developers.tron.network/reference/triggerconstantcontract)

Pinned internal Wallet Core APIs are isolated behind adapters and must be revalidated when dependencies change.
