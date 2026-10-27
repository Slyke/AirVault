use crate::Result;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

pub struct BinaryIdentity {
    pub path: String,
    pub sha256: String,
}
pub fn file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}
/// Reads the executable file afresh. This is a checksum, not an authenticity attestation.
pub fn current_binary() -> Result<BinaryIdentity> {
    let path = std::env::current_exe()?;
    Ok(BinaryIdentity {
        sha256: file_sha256(&path)?,
        path: path.to_string_lossy().into_owned(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hashes_exact_file_bytes_and_recomputes_after_changes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("binary");
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(
            file_sha256(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::write(&file, b"abd").unwrap();
        assert_ne!(
            file_sha256(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(current_binary().unwrap().sha256.len(), 64);
    }
}
