use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "package.json",
    "package-lock.json",
    "index.html",
    "tsconfig.json",
    "vite.config.ts",
    "src",
    "public",
    "crates",
    "build-support",
    "scripts",
    ".cargo",
    "src-tauri/Cargo.toml",
    "src-tauri/build.rs",
    "src-tauri/tauri.conf.json",
    "src-tauri/src",
    "src-tauri/capabilities",
];
fn collect(root: &Path, path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        for item in fs::read_dir(path)? {
            collect(root, &item?.path(), files)?;
        }
    } else {
        files.push(
            path.strip_prefix(root)
                .map_err(io::Error::other)?
                .to_owned(),
        );
    }
    Ok(())
}
pub fn source_hash(root: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    for input in INPUTS {
        collect(root, &root.join(input), &mut files)?;
    }
    files.sort();
    let mut hash = Sha256::new();
    for file in files {
        let name = file.to_string_lossy().replace('\\', "/");
        let bytes = fs::read(root.join(&file))?;
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(format!("{:x}", hash.finalize()))
}
