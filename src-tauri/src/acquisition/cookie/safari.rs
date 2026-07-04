//! Safari cookie provider (macOS only).
//!
//! Safari stores cookies in a proprietary `Cookies.binarycookies` format.
//! Reading the container copy generally requires Full Disk Access (TCC).

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::time::Instant;

use super::{CookieBundle, CookieProvider, ProfileInfo, CLAUDE_DOMAIN_SUFFIX};
use crate::error::CookieError;

pub struct SafariProvider;

impl SafariProvider {
    pub fn new() -> Self {
        Self
    }
}

fn cookie_paths() -> Vec<PathBuf> {
    let home = match std::env::var_os("HOME") {
        Some(h) => PathBuf::from(h),
        None => return Vec::new(),
    };
    vec![
        home.join("Library/Cookies/Cookies.binarycookies"),
        home.join(
            "Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies",
        ),
    ]
}

impl CookieProvider for SafariProvider {
    fn detect_profiles(&self) -> Result<Vec<ProfileInfo>, CookieError> {
        // Safari has a single profile; presence of the cookie file = available.
        let path = cookie_paths().into_iter().find(|p| p.exists());
        match path {
            Some(p) => Ok(vec![ProfileInfo {
                id: "default".into(),
                display_name: "Safari".into(),
                path: p,
            }]),
            None => Err(CookieError::PermissionDenied),
        }
    }

    fn read_claude_cookies(&self, _profile: &str) -> Result<CookieBundle, CookieError> {
        let path = cookie_paths()
            .into_iter()
            .find(|p| p.exists())
            .ok_or(CookieError::PermissionDenied)?;

        let bytes = std::fs::read(&path).map_err(|e| {
            // A permission error here almost always means FDA is missing.
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                CookieError::PermissionDenied
            } else {
                CookieError::Io(e.to_string())
            }
        })?;

        let cookies = parse_binarycookies(&bytes, CLAUDE_DOMAIN_SUFFIX)?;
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

/// Minimal parser for the `Cookies.binarycookies` format.
///
/// Layout: magic "cook", u32(BE) page count, then per-page byte lengths,
/// then pages. Each page: u32(LE) tag, u32(LE) cookie count, offsets, cookies.
/// Each cookie record contains LE offsets to url/name/path/value C-strings.
fn parse_binarycookies(bytes: &[u8], domain_suffix: &str) -> Result<Vec<(String, String)>, CookieError> {
    if bytes.len() < 8 || &bytes[0..4] != b"cook" {
        return Err(CookieError::DecryptFailed);
    }
    let num_pages = be_u32(&bytes[4..8]) as usize;
    let mut cursor = 8usize;

    let mut page_sizes = Vec::with_capacity(num_pages);
    for _ in 0..num_pages {
        if cursor + 4 > bytes.len() {
            return Err(CookieError::DecryptFailed);
        }
        page_sizes.push(be_u32(&bytes[cursor..cursor + 4]) as usize);
        cursor += 4;
    }

    let mut out = Vec::new();
    let mut page_start = cursor;
    for size in page_sizes {
        let page_end = (page_start + size).min(bytes.len());
        let page = &bytes[page_start..page_end];
        parse_page(page, domain_suffix, &mut out);
        page_start = page_end;
    }
    Ok(out)
}

fn parse_page(page: &[u8], domain_suffix: &str, out: &mut Vec<(String, String)>) {
    if page.len() < 8 {
        return;
    }
    let count = le_u32(&page[4..8]) as usize;
    let mut offsets = Vec::with_capacity(count);
    let mut c = 8usize;
    for _ in 0..count {
        if c + 4 > page.len() {
            return;
        }
        offsets.push(le_u32(&page[c..c + 4]) as usize);
        c += 4;
    }
    for off in offsets {
        if off + 40 > page.len() {
            continue;
        }
        let rec = &page[off..];
        let url_off = le_u32(&rec[16..20]) as usize;
        let name_off = le_u32(&rec[20..24]) as usize;
        let _path_off = le_u32(&rec[24..28]) as usize;
        let value_off = le_u32(&rec[28..32]) as usize;

        let url = read_cstr(rec, url_off);
        if !url.contains(domain_suffix) {
            continue;
        }
        let name = read_cstr(rec, name_off);
        let value = read_cstr(rec, value_off);
        if !name.is_empty() {
            out.push((name, value));
        }
    }
}

fn read_cstr(buf: &[u8], start: usize) -> String {
    if start >= buf.len() {
        return String::new();
    }
    let end = buf[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|p| start + p)
        .unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[start..end]).to_string()
}

fn be_u32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}
fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}
