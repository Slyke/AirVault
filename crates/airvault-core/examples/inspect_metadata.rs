use parity_scale_codec::Decode;
fn main() {
    let data = include_bytes!("../tests/fixtures/polkadot-hub-2005000.scale");
    let metadata = frame_metadata::RuntimeMetadataPrefixed::decode(&mut &data[..]).unwrap();
    let frame_metadata::RuntimeMetadata::V15(meta) = metadata.1 else {
        panic!()
    };
    for e in &meta.extrinsic.signed_extensions {
        println!(
            "{}: {:?}",
            e.identifier,
            meta.types.resolve(e.ty.id).map(|t| &t.type_def)
        );
        println!(
            "Additional: {:?}",
            meta.types
                .resolve(e.additional_signed.id)
                .map(|t| &t.type_def)
        );
    }
}
