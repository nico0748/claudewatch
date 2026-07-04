//! Crate-wide error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CookieError {
    #[error("browser not installed")]
    BrowserNotInstalled,
    #[error("profile not found: {0}")]
    ProfileNotFound(String),
    #[error("cookie store is locked (browser running?)")]
    CookieStoreLocked,
    #[error("decryption key unavailable (keychain access denied?)")]
    DecryptKeyUnavailable,
    #[error("failed to decrypt cookie value")]
    DecryptFailed,
    #[error("claude.ai session cookie missing (not logged in)")]
    CookieMissing,
    #[error("permission denied (full disk access required?)")]
    PermissionDenied,
    #[error("io error: {0}")]
    Io(String),
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("unauthorized (session expired)")]
    Unauthorized,
    #[error("rate limited")]
    RateLimited,
    #[error("server error: {0}")]
    Server(u16),
    #[error("network error: {0}")]
    Network(String),
    #[error("failed to parse usage response: {0}")]
    Parse(String),
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Cookie(#[from] CookieError),
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error("account limit reached (max {0})")]
    AccountLimit(usize),
    #[error("storage error: {0}")]
    Storage(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Serializable error for passing across the Tauri IPC boundary.
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
