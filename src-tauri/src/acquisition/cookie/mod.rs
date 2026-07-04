//! Browser cookie extraction abstraction.
//!
//! Each supported browser implements [`CookieProvider`]. Cookie *values* are
//! never persisted — they live only in memory for the duration of a fetch.

pub mod chrome;
pub mod firefox;
#[cfg(target_os = "macos")]
pub mod safari;

use std::path::PathBuf;
use std::time::Instant;

use zeroize::Zeroize;

use crate::domain::account::Browser;
use crate::error::CookieError;

/// A browser profile that may hold a claude.ai session.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProfileInfo {
    pub id: String,
    pub display_name: String,
    #[serde(skip)]
    pub path: PathBuf,
}

/// The set of cookies needed to authenticate against claude.ai.
///
/// Values are zeroized on drop to reduce the window of exposure in memory.
pub struct CookieBundle {
    pub cookies: Vec<(String, String)>,
    pub domain: String,
    pub read_at: Instant,
}

impl CookieBundle {
    /// Serialize into a `Cookie:` header value.
    pub fn header_value(&self) -> String {
        self.cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }
}

impl Drop for CookieBundle {
    fn drop(&mut self) {
        for (name, value) in self.cookies.iter_mut() {
            name.zeroize();
            value.zeroize();
        }
    }
}

/// Names of the cookies claude.ai relies on for authentication.
pub const CLAUDE_COOKIE_NAMES: &[&str] = &["sessionKey", "lastActiveOrg", "__cf_bm"];
pub const CLAUDE_DOMAIN_SUFFIX: &str = "claude.ai";

pub trait CookieProvider: Send + Sync {
    /// Detect installed profiles for this browser.
    fn detect_profiles(&self) -> Result<Vec<ProfileInfo>, CookieError>;

    /// Read the claude.ai session cookies from the given profile.
    fn read_claude_cookies(&self, profile: &str) -> Result<CookieBundle, CookieError>;
}

/// Factory: build the provider for a browser kind.
pub fn provider_for(browser: Browser) -> Box<dyn CookieProvider> {
    match browser {
        Browser::Chrome => Box::new(chrome::ChromiumProvider::chrome()),
        Browser::Brave => Box::new(chrome::ChromiumProvider::brave()),
        Browser::Firefox => Box::new(firefox::FirefoxProvider::new()),
        #[cfg(target_os = "macos")]
        Browser::Safari => Box::new(safari::SafariProvider::new()),
        #[cfg(not(target_os = "macos"))]
        Browser::Safari => Box::new(UnsupportedProvider),
    }
}

/// Fallback used on non-macOS builds when Safari is requested.
#[cfg(not(target_os = "macos"))]
pub struct UnsupportedProvider;

#[cfg(not(target_os = "macos"))]
impl CookieProvider for UnsupportedProvider {
    fn detect_profiles(&self) -> Result<Vec<ProfileInfo>, CookieError> {
        Err(CookieError::BrowserNotInstalled)
    }
    fn read_claude_cookies(&self, _profile: &str) -> Result<CookieBundle, CookieError> {
        Err(CookieError::BrowserNotInstalled)
    }
}
