//! Chromium-family cookie provider (Chrome, Brave).
//!
//! Cookie values are AES-encrypted; the key is derived from a passphrase held
//! in the OS keychain ("Chrome Safe Storage" / "Brave Safe Storage").

use std::path::{Path, PathBuf};
use std::time::Instant;

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;

use super::{CookieBundle, CookieProvider, ProfileInfo, CLAUDE_DOMAIN_SUFFIX};
use crate::error::CookieError;

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// Distinguishes Chrome vs Brave (paths + keychain service names differ).
#[derive(Clone, Copy)]
pub enum Flavor {
    Chrome,
    Brave,
}

pub struct ChromiumProvider {
    flavor: Flavor,
    user_data_dir: Option<PathBuf>,
}

impl ChromiumProvider {
    pub fn chrome() -> Self {
        Self { flavor: Flavor::Chrome, user_data_dir: chrome_user_data_dir() }
    }
    pub fn brave() -> Self {
        Self { flavor: Flavor::Brave, user_data_dir: brave_user_data_dir() }
    }

    fn keychain_service(&self) -> &'static str {
        match self.flavor {
            Flavor::Chrome => "Chrome Safe Storage",
            Flavor::Brave => "Brave Safe Storage",
        }
    }

    fn keychain_account(&self) -> &'static str {
        match self.flavor {
            Flavor::Chrome => "Chrome",
            Flavor::Brave => "Brave",
        }
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn chrome_user_data_dir() -> Option<PathBuf> {
    let h = home()?;
    #[cfg(target_os = "macos")]
    let p = h.join("Library/Application Support/Google/Chrome");
    #[cfg(target_os = "linux")]
    let p = h.join(".config/google-chrome");
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let p = h.join(".config/google-chrome");
    p.exists().then_some(p)
}

fn brave_user_data_dir() -> Option<PathBuf> {
    let h = home()?;
    #[cfg(target_os = "macos")]
    let p = h.join("Library/Application Support/BraveSoftware/Brave-Browser");
    #[cfg(target_os = "linux")]
    let p = h.join(".config/BraveSoftware/Brave-Browser");
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let p = h.join(".config/BraveSoftware/Brave-Browser");
    p.exists().then_some(p)
}

impl CookieProvider for ChromiumProvider {
    fn detect_profiles(&self) -> Result<Vec<ProfileInfo>, CookieError> {
        let root = self.user_data_dir.clone().ok_or(CookieError::BrowserNotInstalled)?;
        let mut profiles = Vec::new();
        for name in ["Default"].iter().map(|s| s.to_string()).chain(
            (1..=9).map(|i| format!("Profile {i}")),
        ) {
            let dir = root.join(&name);
            if dir.join("Cookies").exists() || dir.join("Network/Cookies").exists() {
                profiles.push(ProfileInfo {
                    id: name.clone(),
                    display_name: name,
                    path: dir,
                });
            }
        }
        if profiles.is_empty() {
            return Err(CookieError::ProfileNotFound("no chromium profile".into()));
        }
        Ok(profiles)
    }

    fn read_claude_cookies(&self, profile: &str) -> Result<CookieBundle, CookieError> {
        let root = self.user_data_dir.clone().ok_or(CookieError::BrowserNotInstalled)?;
        let dir = root.join(profile);
        let db = if dir.join("Network/Cookies").exists() {
            dir.join("Network/Cookies")
        } else {
            dir.join("Cookies")
        };
        if !db.exists() {
            return Err(CookieError::ProfileNotFound(profile.to_string()));
        }

        let key = derive_key(self.keychain_service(), self.keychain_account())?;
        let raw = read_encrypted_cookies(&db)?;

        let mut cookies = Vec::new();
        for (name, enc) in raw {
            match decrypt_value(&enc, &key) {
                Ok(value) => cookies.push((name, value)),
                Err(_) => continue, // skip undecryptable entries
            }
        }
        if cookies.is_empty() {
            return Err(CookieError::CookieMissing);
        }
        Ok(CookieBundle {
            cookies,
            domain: format!(".{CLAUDE_DOMAIN_SUFFIX}"),
            read_at: Instant::now(),
        })
    }
}

/// Fetch the Safe Storage passphrase and derive the AES key via PBKDF2.
fn derive_key(service: &str, account: &str) -> Result<[u8; 16], CookieError> {
    let passphrase = keychain_passphrase(service, account)?;
    // Chromium uses 1003 iterations on macOS, 1 on Linux with "peanuts".
    #[cfg(target_os = "macos")]
    let iterations = 1003u32;
    #[cfg(not(target_os = "macos"))]
    let iterations = 1u32;

    let mut key = [0u8; 16];
    pbkdf2_hmac::<Sha1>(passphrase.as_bytes(), b"saltysalt", iterations, &mut key);
    Ok(key)
}

fn keychain_passphrase(service: &str, account: &str) -> Result<String, CookieError> {
    #[cfg(target_os = "macos")]
    {
        let entry = keyring::Entry::new(service, account)
            .map_err(|_| CookieError::DecryptKeyUnavailable)?;
        entry.get_password().map_err(|_| CookieError::DecryptKeyUnavailable)
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Try Secret Service; fall back to the well-known default passphrase.
        let _ = (service, account);
        match keyring::Entry::new(service, account).and_then(|e| e.get_password()) {
            Ok(p) => Ok(p),
            Err(_) => Ok("peanuts".to_string()),
        }
    }
}

/// Decrypt a Chromium `encrypted_value` (v10/v11 -> AES-128-CBC).
fn decrypt_value(enc: &[u8], key: &[u8; 16]) -> Result<String, CookieError> {
    if enc.len() < 3 {
        return Err(CookieError::DecryptFailed);
    }
    let prefix = &enc[..3];
    if prefix != b"v10" && prefix != b"v11" {
        // Unencrypted (older) value or unsupported scheme.
        return String::from_utf8(enc.to_vec()).map_err(|_| CookieError::DecryptFailed);
    }
    let iv = [0x20u8; 16]; // 16 spaces
    let ct = &enc[3..];
    let pt = Aes128CbcDec::new(key.into(), &iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(ct)
        .map_err(|_| CookieError::DecryptFailed)?;
    // Newer Chrome prefixes 32 bytes of SHA256(domain); strip non-UTF8 lead if present.
    let s = String::from_utf8_lossy(&pt).trim_start().to_string();
    Ok(s)
}

fn read_encrypted_cookies(db: &Path) -> Result<Vec<(String, Vec<u8>)>, CookieError> {
    let tmp = copy_to_temp(db).map_err(|e| CookieError::Io(e.to_string()))?;
    let conn = rusqlite::Connection::open_with_flags(
        &tmp,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|_| CookieError::CookieStoreLocked)?;

    let mut stmt = conn
        .prepare("SELECT name, encrypted_value FROM cookies WHERE host_key LIKE ?1")
        .map_err(|e| CookieError::Io(e.to_string()))?;
    let like = format!("%{CLAUDE_DOMAIN_SUFFIX}");
    let rows = stmt
        .query_map([like], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(|e| CookieError::Io(e.to_string()))?;

    let mut out = Vec::new();
    for r in rows.flatten() {
        out.push(r);
    }
    let _ = std::fs::remove_file(&tmp);
    Ok(out)
}

/// Copy a (possibly locked, WAL-mode) sqlite DB to a temp file for reading.
pub fn copy_to_temp(db: &Path) -> std::io::Result<PathBuf> {
    let mut tmp = std::env::temp_dir();
    let unique = format!(
        "claudewatch-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    tmp.push(unique);
    std::fs::copy(db, &tmp)?;
    for ext in ["-wal", "-shm"] {
        let mut side = db.as_os_str().to_owned();
        side.push(ext);
        let side = PathBuf::from(side);
        if side.exists() {
            let mut dst = tmp.as_os_str().to_owned();
            dst.push(ext);
            let _ = std::fs::copy(&side, PathBuf::from(dst));
        }
    }
    Ok(tmp)
}
