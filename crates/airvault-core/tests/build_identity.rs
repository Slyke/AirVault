#[path = "../../../build-support/fingerprint.rs"]
mod fingerprint;
#[test]
fn source_identity_covers_uncommitted_and_new_input_files() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    let file = root.path().join("src/app.ts");
    std::fs::write(&file, "original").unwrap();
    let first = fingerprint::source_hash(root.path()).unwrap();
    assert_eq!(first, fingerprint::source_hash(root.path()).unwrap());
    std::fs::write(&file, "edited").unwrap();
    let second = fingerprint::source_hash(root.path()).unwrap();
    assert_ne!(first, second);
    std::fs::write(root.path().join("src/new.ts"), "new input").unwrap();
    assert_ne!(second, fingerprint::source_hash(root.path()).unwrap());
}
