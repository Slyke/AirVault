use airvault_core::{
    bitcoin as adapter,
    model::{Mode, Network},
    protocol,
    session::Session,
};
use bitcoin::{
    absolute, consensus, psbt::Psbt, transaction, Address, Amount, OutPoint, ScriptBuf, Sequence,
    Transaction, TxIn, TxOut, Witness,
};
use std::{collections::HashMap, str::FromStr};
const PASSWORD: &str = "public-export-test";
fn fixture(dir: &std::path::Path) -> Session {
    let mut s = Session::new(dir.join("signer.avvault"));
    s.create(PASSWORD, Mode::Offline).unwrap();
    s.mutate(|v|{
        let id=v.import_phrase("Public test vector","abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",None)?;
        for index in 0..2 {v.derive_account(id,"Test",Network::Regtest,&format!("m/84'/1'/0'/0/{index}"),None)?;}
        Ok(())
    }).unwrap();
    s
}
#[test]
fn signed_single_psbt_and_raw_exports_round_trip_and_reject_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = fixture(dir.path());
    let accounts = s.snapshot().accounts;
    let address = Address::from_str(&accounts[0].address)
        .unwrap()
        .require_network(bitcoin::Network::Regtest)
        .unwrap();
    let previous = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::null(),
            script_sig: ScriptBuf::from_bytes(vec![1, 1]),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![
            TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: address.script_pubkey()
            };
            2
        ],
    };
    let request = adapter::construct(
        Network::Regtest,
        (0..2)
            .map(|vout| adapter::Utxo {
                transaction_hex: hex::encode(consensus::serialize(&previous)),
                vout,
            })
            .collect(),
        vec![adapter::Recipient {
            address: accounts[1].address.clone(),
            satoshis: 50_000,
        }],
        &accounts[0].address,
        1000,
        &accounts,
    )
    .unwrap();
    let id = s
        .import_psbt(&request.validate().unwrap(), Network::Regtest)
        .unwrap();
    assert_eq!(
        s.import_psbt(&request.validate().unwrap(), Network::Regtest)
            .unwrap(),
        id
    );
    assert_eq!(s.snapshot().pending.len(), 1);
    assert!(s.transaction_bytes(id).is_err());
    let old = s.review(id).unwrap();
    s.import_psbt(&request.validate().unwrap(), Network::Regtest)
        .unwrap();
    assert!(s
        .sign(old.token, true, PASSWORD, &HashMap::new(), &[])
        .is_err());
    let review = s.review(id).unwrap();
    let response = s
        .sign(review.token, true, PASSWORD, &HashMap::new(), &[])
        .unwrap();
    let psbt = Psbt::deserialize(&s.psbt_bytes(id).unwrap()).unwrap();
    assert!(psbt.inputs.iter().all(|i| i.partial_sigs.len() == 1
        && i.non_witness_utxo.is_some()
        && i.bip32_derivation.len() == 1));
    assert!(psbt.outputs.iter().all(|o| o.bip32_derivation.len() == 1));
    let mut finalized = psbt.unsigned_tx.clone();
    for (txin, input) in finalized.input.iter_mut().zip(&psbt.inputs) {
        let (key, sig) = input.partial_sigs.iter().next().unwrap();
        txin.witness.push(sig.to_vec());
        txin.witness.push(key.to_bytes());
    }
    assert_eq!(
        consensus::serialize(&finalized),
        s.transaction_bytes(id).unwrap()
    );
    assert_eq!(
        response.payload.decode().unwrap(),
        s.transaction_bytes(id).unwrap()
    );
    s.lock();
    s.unlock(PASSWORD).unwrap();
    assert_eq!(s.psbt_bytes(id).unwrap(), psbt.serialize());
    let mut proxy = Session::new(dir.path().join("proxy.avvault"));
    proxy.create(PASSWORD, Mode::Proxy).unwrap();
    proxy.import_public(&s.public_bytes().unwrap()).unwrap();
    let original = s.snapshot().pending[0].clone();
    proxy
        .import_request(&serde_json::to_vec(&original).unwrap())
        .unwrap();
    proxy
        .import_response(&serde_json::to_vec(&response).unwrap())
        .unwrap();
    assert_eq!(proxy.psbt_bytes(id).unwrap(), psbt.serialize());
    assert_eq!(
        proxy.transaction_bytes(id).unwrap(),
        consensus::serialize(&finalized)
    );
    // Recomputed outer hashes cannot make substituted transaction bytes exportable.
    s.mutate(|v| {
        let r = &mut v.data.responses[0];
        let mut tx = finalized.clone();
        tx.output[0].value = Amount::from_sat(1);
        let raw = consensus::serialize(&tx);
        r.payload = protocol::Payload::new(&raw);
        r.payload_sha256 = protocol::digest(&raw);
        Ok(())
    })
    .unwrap();
    assert!(s.psbt_bytes(id).is_err());
    assert!(s.transaction_bytes(id).is_err());
    s.lock();
    assert!(s.psbt_bytes(id).is_err());
    assert!(s.transaction_bytes(id).is_err());
}
