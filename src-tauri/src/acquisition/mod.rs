//! Data acquisition (方式A): read browser cookies, then query Claude.

pub mod claude_client;
pub mod cookie;

pub use claude_client::ClaudeClient;
pub use cookie::{CookieBundle, CookieProvider, ProfileInfo, provider_for};
