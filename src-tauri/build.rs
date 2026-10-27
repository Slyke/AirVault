#[path = "../build-support/fingerprint.rs"]
mod fingerprint;
use std::{
    env, fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
fn git(root: &std::path::Path, args: &[&str]) -> Option<String> {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
}
fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.parent().unwrap();
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    for file in ["package.json", "src-tauri/tauri.conf.json"] {
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(file)).unwrap()).unwrap();
        assert_eq!(
            json["version"].as_str(),
            Some(version.as_str()),
            "Application versions must agree"
        );
    }
    for input in fingerprint::INPUTS {
        println!("cargo:rerun-if-changed={}", root.join(input).display());
    }
    if let Some(dir) = git(root, &["rev-parse", "--absolute-git-dir"]) {
        for entry in ["HEAD", "refs", "packed-refs", "index"] {
            println!(
                "cargo:rerun-if-changed={}",
                PathBuf::from(&dir).join(entry).display()
            );
        }
    }
    let hash = git(root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = git(root, &["status", "--porcelain"]).is_none_or(|s| !s.is_empty());
    let source = fingerprint::source_hash(root).expect("Cannot fingerprint source inputs");
    let built = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let target = env::var("TARGET").unwrap();
    println!("cargo:rustc-env=AIRVAULT_BUILD_HASH={hash}");
    println!("cargo:rustc-env=AIRVAULT_SOURCE_HASH={source}");
    println!("cargo:rustc-env=AIRVAULT_BUILT_AT={built}");
    println!("cargo:rustc-env=AIRVAULT_BUILD_TARGET={target}");
    println!("cargo:rustc-env=AIRVAULT_DIRTY={dirty}");
    let metadata = serde_json::json!({"version":version,"buildHash":hash,"sourceHash":source,"builtAt":built,"target":target,"dirty":dirty});
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(
        out.join("build-info.json"),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();
    // Keep the manifest beside this target's executable for the release packager.
    fs::write(
        out.ancestors()
            .nth(3)
            .unwrap()
            .join("airvault-build-info.json"),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();
    tauri_build::build();
}
