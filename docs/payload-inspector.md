# Payload inspector

Available in Offline, Proxy and Online modes. Open **Payload inspector** in the sidebar, open **Inspect payload** on a signing request, or expand it inside transaction review. Importing an exchange file or PSBT opens its inspection, including when the subsequent vault import is rejected.

## Basic use

1. Select **Open any file**. No extension filter is applied. Files may be up to 8 MiB.
2. Alternatively, paste hex, standard Base64 or an AirVault JSON envelope and select **Inspect text**.
3. Use **Byte document** to switch between the exact original file, its decoded payload and embedded PSBT transactions.
4. Hover over hex or ASCII bytes to highlight the same field in both views. The helper describes the field, its value and what should be there. Click to pin; use **Unpin** or Escape to release.
5. Use the field search, byte paging and decimal or 0x hexadecimal offsets to navigate. Arrow keys move the keyboard cursor across bytes.
6. Expand checks, especially failures. A failure anywhere in the decoded documents is indicated above the view.
7. Compare destination, amount, network and hashes against a trusted independent source.

Check the request on the Proxy before exporting, again on the Offline signer, and inspect the signed response before broadcasting. The request view can select the original envelope, the current PSBT including collected signatures, or a stored signed response. Inspection never approves, signs, sends, registers or modifies a transaction.

Stored request/response views inspect the canonical export bytes. Use file inspection when comparing the exact whitespace/encoding of an imported JSON file.

The file hash always hashes the exact bytes selected, including whitespace in encoded text. Offsets belong to the selected document. Base64 and envelope decoding produce new documents whose offsets start at zero.

## Decoders and checks

| Format | Decoding and validation |
| --- | --- |
| Bitcoin transactions | Legacy and SegWit fields, input/output scripts with assembly, witness items, canonical CompactSize, exact consumption, output money bounds, duplicate inputs, txid and wtxid |
| PSBTv0 | Global/input/output maps, record lengths and uniqueness, native PSBT decoding, nested unsigned/previous transactions, previous txid and vout binding, witness UTXO agreement, supplied redeem/witness script commitments, fee from supplied consistent UTXOs |
| Bitcoin P2P v1 | Mainnet, testnet3, testnet4, default signet and regtest magic; command padding, declared lengths and first four bytes of double SHA-256 per message; concatenated messages supported |
| Common P2P bodies | tx, block, version, ping/pong, feefilter, sendcmpct, inventory/getdata/notfound, headers, getheaders/getblocks and empty control messages. Blocks check transaction merkle root and witness commitment |
| EVM | Legacy, EIP-2930/type 1 and EIP-1559/type 2 transaction RLP, canonical lengths and integers, field counts, recipient size, access-list layout, Keccak-256, function selector and ABI-word segmentation |
| AirVault envelopes | Native request/response schema and decoded-payload SHA-256; Bitcoin response txid comparison; nested payload decoding. Other chain requests are inspectable as typed JSON |
| Other files | Exact bytes, ASCII and SHA-256, with an explicit unrecognized-format result |

An explicit decoder can inspect a damaged header that automatic detection cannot recognize. An unknown PSBT version retains the decoded global map and opaque remaining bytes; it is not reported as a validated PSBTv0.

## Verification boundaries

- Raw Bitcoin transactions, PSBTs and EVM transactions have no embedded file checksum. A computed identifier or file hash is useful only when compared with an independently trusted value.
- P2P and envelope checksums detect corruption. They do not authenticate peers, the envelope sender, destination ownership or spending intent.
- Script execution, signature verification/recovery, contract execution, current balances/nonces, spent-output status, chain membership and proof of work are outside this read-only decoder.
- A witness-only PSBT UTXO is supplied metadata, not an authenticated previous transaction. Fee reporting does not establish that those inputs exist or are unspent.
- Response requestDigest binding needs the original request. Existing AirVault signing review and independent broadcast verification retain that responsibility.
- Unsupported P2P commands remain opaque, even when transport framing/checksum passes. Encrypted BIP324 streams and PCAP/TCP/IP containers are not decoded. Supply extracted, reassembled application-message bytes.
- Parsing is capped at 24,000 detailed fields per document, 10,000 transaction inputs/outputs, 1,024 P2P messages, 32 documents and bounded nested children. The complete original bytes remain available; remaining bytes are marked undecoded when a limit is reached.

## Implementation

The Rust inspector module has no vault or network dependency. The inspect_file command uses bounded local reads and a blocking worker; inspect_text runs the same parser. The inspect_exchange command copies a request, stored response or current PSBT from the unlocked session without changing approval or wallet state. All three native commands are registered in both feature builds. No decoder parameters or payload content are logged.

The UI renders bounded pages of 256 bytes, searches named fields and uses function-group background accents from the styleguide. Semantic colors indicate check results, focus and selection. All data is rendered as text; embedded markup and scripts are inert. Test fixtures and automation commands live solely in the separate native test bridge, excluded from production.

## Protocol references

- [Bitcoin transaction serialization](https://developer.bitcoin.org/reference/transactions.html)
- [BIP144 witness serialization](https://bips.dev/144/)
- [BIP174 PSBT](https://bips.dev/174/)
- [Bitcoin P2P message reference](https://developer.bitcoin.org/reference/p2p_networking.html)
- [Bitcoin Core message header](https://github.com/bitcoin/bitcoin/blob/master/src/protocol.h)
- [EVM RLP](https://ethereum.org/developers/docs/data-structures-and-encoding/rlp/)
- [EIP1559 transaction fields](https://eips.ethereum.org/EIPS/eip-1559)
