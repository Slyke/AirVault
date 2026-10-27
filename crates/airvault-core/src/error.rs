use serde::Serialize;
pub type Result<T> = std::result::Result<T, Error>;
/// Errors contain static, non-secret descriptions only. Never forward parser or crypto errors.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "code", content = "message")]
pub enum Error {
    #[error("Vault is locked.")]
    Locked,
    #[error("Password is incorrect or encrypted data is damaged.")]
    Authentication,
    #[error("Invalid input: {0}")]
    Invalid(&'static str),
    #[error("Unsupported operation: {0}")]
    Unsupported(&'static str),
    #[error("This operation is unavailable in the selected mode.")]
    Mode,
    #[error("Explicit confirmation is required.")]
    Approval,
    #[error("Requested item was not found.")]
    NotFound,
    #[error("This request already exists or was already signed.")]
    Duplicate,
    #[error("Storage operation failed. No secret details were logged.")]
    Storage,
    #[error("Signing or independent verification failed.")]
    Verification,
    #[error("Network request failed. Check the configured provider.")]
    Network,
}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::Storage
    }
}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::Invalid("JSON does not match the documented schema")
    }
}
