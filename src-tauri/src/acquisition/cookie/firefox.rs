//! Firefox cookie provider. Values in `cookies.sqlite` are stored in plaintext.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::{
    CookieBundle, CookieProvider, ProfileInfo, CLAUDE_DOMAIN_SUFFIX,
};
use crate::error::CookieError;

pub struct FirefoxProvider {
    root: Option<PathBuf>,
}

impl FirefoxProvider {
    pub fn new() -> Self {
        Self { root: firefox_root() }
    }
}

/// Base directory holding `profiles.ini` and the profile folders.
fn firefox_root() -> Option<PathBuf> {
    let home = dirs_home()?;
    #[cfg(target_os = "macos")]
    let p = home.join("Library/Application Support/Firefox");
    #[cfg(target_os = "linux")]
    let p = home.join(".mozilla/firefox");
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let p = home.join(".mozilla/firefox");
    p.exists().then_some(p)
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

impl CookieProvider for FirefoxProvider {
    fn detect_profiles(&self) -> Result<Vec<ProfileInfo>, CookieError> {
        let root = self.root.clone().ok_or(CookieError::BrowserNotInstalled)?;
        let ini = root.join("profiles.ini");
        let text = std::fs::read_to_string(&ini)
            .map_err(|e| CookieError::Io(e.to_string()))?;

        let mut profiles = Vec::new();
        let mut cur_name: Option<String> = None;
        let mut cur_path: Option<String> = None;
        let mut is_relative = true;

        let flush = |profiles: &mut Vec<ProfileInfo>,
                     name: &Option<String>,
                     path: &Option<String>,
                     relative: bool| {
            if let (Some(name), Some(path)) = (name, path) {
                let full = if relative { root.join(path) } else { PathBuf::from(path) };
                if full.join("cookies.sqlite").exists() {
                    profiles.push(ProfileInfo {
                        id: path.clone(),
                        display_name: name.clone(),
                        path: full,
                    });
                }
            }
        };

        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                flush(&mut profiles, &cur_name, &cur_path, is_relative);
                cur_name = None;
                cur_path = None;
                is_relative = true;
            } else if let Some(v) = line.strip_prefix("Name=") {
                cur_name = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("Path=") {
                cur_path = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("IsRelative=") {
                is_relative = v.trim() != "0";
            }
        }
        flush(&mut profiles, &cur_name, &cur_path, is_relative);

        if profiles.is_empty() {
            return Err(CookieError::ProfileNotFound("no firefox profile".into()));
        }
        Ok(profiles)
    }

    fn read_claude_cookies(&self, profile: &str) -> Result<CookieBundle, CookieError> {
        let root = self.root.clone().ok_or(CookieError::BrowserNotInstalled)?;
        let db = root.join(profile).join("cookies.sqlite");
        if !db.exists() {
            return Err(CookieError::ProfileNotFound(profile.to_string()));
        }
        let cookies = read_moz_cookies(&db)?;
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

/// Copy the DB to a temp file (avoiding locks) and read claude.ai cookies.
fn read_moz_cookies(db: &Path) -> Result<Vec<(String, String)>, CookieError> {
    let tmp = crate::acquisition::cookie::chrome::copy_to_temp(db)
        .map_err(|e| CookieError::Io(e.to_string()))?;
    let conn = rusqlite::Connection::open_with_flags(
        &tmp,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|_| CookieError::CookieStoreLocked)?;

    let mut stmt = conn
        .prepare("SELECT name, value FROM moz_cookies WHERE host LIKE ?1")
        .map_err(|e| CookieError::Io(e.to_string()))?;
    let like = format!("%{CLAUDE_DOMAIN_SUFFIX}");
    let rows = stmt
        .query_map([like], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| CookieError::Io(e.to_string()))?;

    let mut out = Vec::new();
    for r in rows {
        if let Ok(pair) = r {
            out.push(pair);
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Ok(out)
}
