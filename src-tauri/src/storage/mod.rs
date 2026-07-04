//! Persistence for configuration and (optionally) usage history.
//!
//! Cookie values and decryption keys are NEVER written here.

pub mod config;

pub use config::{AppConfig, ConfigStore};
