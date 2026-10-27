use airvault_core::{
    inspector::{self, Document, Report},
    protocol,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use bitcoin::{
    absolute,
    blockdata::constants::genesis_block,
    consensus,
    p2p::{
        message::{NetworkMessage, RawNetworkMessage},
        Magic,
    },
    psbt::Psbt,
    transaction, Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness,
};
use sha2::{Digest, Sha256};

fn report(bytes: &[u8]) -> Report {
    let r = inspector::inspect(bytes, "auto").unwrap();
    coverage(&r);
    r
}
fn coverage(r: &Report) {
    for d in &r.documents {
        let mut end = 0;
        for f in &d.fields {
            assert_eq!(f.offset, end, "{}: {}", d.name, f.name);
            assert!(f.length > 0);
            end = f.offset + f.length;
        }
        assert_eq!(end, d.hex.len() / 2, "all bytes must remain inspectable");
    }
}
fn failed(d: &Document) -> bool {
    d.checks.iter().any(|c| c.status == "fail")
}
fn has(d: &Document, name: &str, status: &str) -> bool {
    d.checks
        .iter()
        .any(|c| c.name.contains(name) && c.status == status)
}
fn sample() -> (Transaction, Psbt) {
    let previous = genesis_block(bitcoin::Network::Bitcoin).txdata[0].clone();
    let tx = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::new(previous.compute_txid(), 0),
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(4_999_999_000),
            script_pubkey: previous.output[0].script_pubkey.clone(),
        }],
    };
    let mut psbt = Psbt::from_unsigned_tx(tx.clone()).unwrap();
    psbt.inputs[0].non_witness_utxo = Some(previous);
    (tx, psbt)
}
#[test]
fn transactions_preserve_ranges_and_identifiers_for_binary_hex_and_base64() {
    let tx = genesis_block(bitcoin::Network::Bitcoin).txdata[0].clone();
    let raw = consensus::serialize(&tx);
    for bytes in [
        raw.clone(),
        hex::encode(&raw).into_bytes(),
        format!(" \n{}\n", STANDARD.encode(&raw)).into_bytes(),
    ] {
        let r = report(&bytes);
        let d = r.documents.last().unwrap();
        assert_eq!(r.source_sha256, hex::encode(Sha256::digest(&bytes)));
        assert_eq!(d.protocol, "Bitcoin transaction");
        assert!(!failed(d));
        assert!(d
            .properties
            .iter()
            .any(|(n, v)| n == "txid" && v == &tx.compute_txid().to_string()));
        assert!(d
            .fields
            .iter()
            .any(|f| f.name.contains("scriptPubKey") && f.explanation.contains("Script")));
        assert!(has(d, "Checksum", "info"));
    }
    let mut witness = sample().0;
    witness.input[0].witness.push([1, 2, 3]);
    let r = report(&consensus::serialize(&witness));
    assert!(!failed(&r.documents[0]));
    assert!(r.documents[0]
        .fields
        .iter()
        .any(|f| f.name == "Input 1 · witness 1"));
}
#[test]
fn rejects_noncanonical_truncated_duplicate_and_overflowing_transactions() {
    let (mut tx, _) = sample();
    let raw = consensus::serialize(&tx);
    for end in 4..raw.len() {
        let r = inspector::inspect(&raw[..end], "bitcoin-tx").unwrap();
        coverage(&r);
        assert!(failed(&r.documents[0]), "truncation {end}");
    }
    let mut trailing = raw.clone();
    trailing.push(0);
    assert!(failed(&report(&trailing).documents[0]));
    let mut nonminimal = raw[..4].to_vec();
    nonminimal.extend([0xfd, 1, 0]);
    nonminimal.extend_from_slice(&raw[5..]);
    assert!(failed(&report(&nonminimal).documents[0]));
    tx.input.push(tx.input[0].clone());
    assert!(has(
        &report(&consensus::serialize(&tx)).documents[0],
        "Input uniqueness",
        "fail"
    ));
    tx.output[0].value = Amount::from_sat(u64::MAX);
    assert!(failed(&report(&consensus::serialize(&tx)).documents[0]));
}
#[test]
fn psbt_checks_maps_previous_outputs_scripts_and_envelope_hash() {
    let (_, mut psbt) = sample();
    let r = report(&psbt.serialize());
    let d = &r.documents[0];
    assert!(!failed(d));
    assert!(has(d, "UTXO binding", "pass"));
    assert!(r.documents.len() >= 3);
    assert!(d
        .properties
        .iter()
        .any(|(n, v)| n.starts_with("Fee") && v == "1000"));
    psbt.inputs[0].non_witness_utxo.as_mut().unwrap().output[0].value = Amount::from_sat(1);
    assert!(has(
        &report(&psbt.serialize()).documents[0],
        "UTXO binding",
        "fail"
    ));
    let (_, mut psbt) = sample();
    psbt.inputs[0].witness_utxo = Some(TxOut {
        value: Amount::from_sat(1),
        script_pubkey: ScriptBuf::new(),
    });
    assert!(has(
        &report(&psbt.serialize()).documents[0],
        "UTXO consistency",
        "fail"
    ));
    psbt.inputs[0].witness_utxo = None;
    psbt.inputs[0].witness_script = Some(ScriptBuf::from_bytes(vec![0x51]));
    assert!(has(
        &report(&psbt.serialize()).documents[0],
        "witness script hash",
        "fail"
    ));
    let (_, mut invalid_values) = sample();
    invalid_values.inputs[0].non_witness_utxo = None;
    invalid_values.inputs[0].witness_utxo = Some(TxOut {
        value: Amount::from_sat(u64::MAX),
        script_pubkey: ScriptBuf::new(),
    });
    assert!(has(
        &report(&invalid_values.serialize()).documents[0],
        "Input money range",
        "fail"
    ));
    let (_, psbt) = sample();
    let mut req = protocol::Request::bitcoin(
        airvault_core::model::Network::Bitcoin,
        &psbt.serialize(),
        "Public fixture".into(),
    );
    let r = report(&serde_json::to_vec_pretty(&req).unwrap());
    assert!(has(&r.documents[0], "Payload SHA-256", "pass"));
    assert!(has(&r.documents[0], "Envelope schema", "pass"));
    assert_eq!(r.documents[1].protocol, "Bitcoin PSBT");
    req.payload_sha256 = "00".repeat(32);
    let r = report(&serde_json::to_vec(&req).unwrap());
    assert!(has(&r.documents[0], "Payload SHA-256", "fail"));
    assert_eq!(
        r.documents[1].protocol, "Bitcoin PSBT",
        "damaged envelopes still expose payload bytes"
    );
}
#[test]
fn psbt_rejects_duplicate_keys_noncanonical_lengths_and_trailing_maps() {
    let (_, psbt) = sample();
    let b = psbt.serialize();
    let entry_end = 8 + consensus::serialize(&psbt.unsigned_tx).len();
    assert_eq!(b[5], 1);
    assert_eq!(b[6], 0);
    assert!(b[7] < 253);
    let mut duplicate = b[..entry_end].to_vec();
    duplicate.extend_from_slice(&b[5..entry_end]);
    duplicate.extend_from_slice(&b[entry_end..]);
    assert!(failed(&report(&duplicate).documents[0]));
    let mut trailing = b.clone();
    trailing.push(0);
    assert!(failed(&report(&trailing).documents[0]));
    let mut length = b[..5].to_vec();
    length.extend([0xfd, 1, 0]);
    length.extend_from_slice(&b[6..]);
    assert!(failed(&report(&length).documents[0]));
    for end in [1, 4, 5, 7, b.len() - 1] {
        let r = inspector::inspect(&b[..end], "psbt").unwrap();
        coverage(&r);
        assert!(failed(&r.documents[0]));
    }
}
#[test]
fn p2p_checks_every_message_checksum_length_and_nested_transaction() {
    let (tx, _) = sample();
    let ping = consensus::serialize(&RawNetworkMessage::new(
        Magic::BITCOIN,
        NetworkMessage::Ping(42),
    ));
    let raw = consensus::serialize(&RawNetworkMessage::new(
        Magic::TESTNET4,
        NetworkMessage::Tx(tx),
    ));
    let digest = Sha256::digest(Sha256::digest(&raw[24..]));
    assert_eq!(&raw[20..24], &digest[..4]);
    let mut stream = ping;
    stream.extend(&raw);
    let r = report(&stream);
    assert!(!failed(&r.documents[0]));
    assert_eq!(
        r.documents[0]
            .checks
            .iter()
            .filter(|c| c.name.contains("checksum") && c.status == "pass")
            .count(),
        2
    );
    assert_eq!(r.documents[1].protocol, "Bitcoin transaction");
    let mut changed = raw.clone();
    changed[20] ^= 1;
    assert!(has(&report(&changed).documents[0], "checksum", "fail"));
    let mut too_long = raw.clone();
    too_long[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(failed(&report(&too_long).documents[0]));
    let mut padding = raw.clone();
    padding[7] = 0x78;
    assert!(failed(&report(&padding).documents[0]));
    assert!(failed(&report(&raw[..raw.len() - 1]).documents[0]));
}
#[test]
fn p2p_block_integrity_and_unknown_commands_remain_distinct() {
    let block = genesis_block(bitcoin::Network::Bitcoin);
    let bytes = consensus::serialize(&RawNetworkMessage::new(
        Magic::BITCOIN,
        NetworkMessage::Block(block),
    ));
    let r = report(&bytes);
    assert!(!failed(&r.documents[0]));
    assert!(has(&r.documents[0], "merkle root", "pass"));
    let mut bad = bytes.clone();
    bad[24 + 36] ^= 1;
    let checksum = Sha256::digest(Sha256::digest(&bad[24..]));
    bad[20..24].copy_from_slice(&checksum[..4]);
    let r = report(&bad);
    assert!(has(&r.documents[0], "checksum", "pass"));
    assert!(has(&r.documents[0], "merkle root", "fail"));
    let mut unknown = bytes[..24].to_vec();
    unknown[4..16].copy_from_slice(b"future\0\0\0\0\0\0");
    unknown[16..20].copy_from_slice(&3u32.to_le_bytes());
    let hash = Sha256::digest(Sha256::digest([1, 2, 3]));
    unknown[20..24].copy_from_slice(&hash[..4]);
    unknown.extend([1, 2, 3]);
    let r = report(&unknown);
    assert!(has(&r.documents[0], "checksum", "pass"));
    assert!(has(&r.documents[0], "Command payload", "info"));
}
#[test]
fn evm_rlp_fields_and_abi_words_are_decoded_without_claiming_execution() {
    let mut s = rlp::RlpStream::new_list(9);
    s.append(&1u8)
        .append(&1u8)
        .append(&21000u32)
        .append(&vec![0x11u8; 20])
        .append(&7u8);
    let data = [vec![0xa9, 0x05, 0x9c, 0xbb], vec![0u8; 64]].concat();
    s.append(&data).append(&37u8).append(&1u8).append(&1u8);
    let raw = s.out().to_vec();
    let r = report(&raw);
    let d = r.documents.last().unwrap();
    assert_eq!(d.protocol, "EVM transaction");
    assert!(!failed(d));
    assert!(d
        .fields
        .iter()
        .any(|f| f.name == "Function selector" && f.value == "a9059cbb"));
    assert!(d.fields.iter().any(|f| f.name == "Call data · ABI word 2"));
    for kind in [1u8, 2] {
        let mut typed = rlp::RlpStream::new_list(if kind == 1 { 8 } else { 9 });
        typed.append(&1u8).append(&0u8).append(&1u8);
        if kind == 2 {
            typed.append(&2u8);
        }
        typed
            .append(&21000u32)
            .append(&vec![0x11u8; 20])
            .append(&7u8)
            .append_empty_data()
            .begin_list(0);
        let mut encoded = vec![kind];
        encoded.extend_from_slice(&typed.out());
        let typed_report = report(&encoded);
        assert!(!failed(&typed_report.documents[0]));
    }
    let mut invalid = rlp::RlpStream::new_list(6);
    invalid
        .append(&vec![0u8])
        .append(&1u8)
        .append(&21000u32)
        .append(&vec![0x11u8; 20])
        .append(&7u8)
        .append_empty_data();
    assert!(failed(
        &inspector::inspect(&invalid.out(), "evm").unwrap().documents[0]
    ));
}
#[test]
fn arbitrary_and_hostile_files_stay_bounded_and_do_not_panic() {
    for bytes in [
        vec![],
        b"not a transaction".to_vec(),
        vec![0xff; 128],
        b"{broken".to_vec(),
        b"{\"hello\":\"<script>alert(1)</script>\"}".to_vec(),
    ] {
        let r = report(&bytes);
        assert!(!r.documents.is_empty());
        assert!(r.documents.len() <= 32);
    }
    let mut state = 12345678u32;
    for size in 0..400usize {
        let b: Vec<_> = (0..size)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        for hint in ["auto", "bitcoin-tx", "psbt", "bitcoin-p2p", "evm"] {
            coverage(&inspector::inspect(&b, hint).unwrap());
        }
    }
    let huge = vec![0u8; inspector::MAX_FILE_BYTES + 1];
    assert!(inspector::inspect(&huge, "auto").is_err());
    assert!(inspector::inspect(&[], "not-a-decoder").is_err());
}
