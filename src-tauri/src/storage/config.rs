//! App configuration: accounts (references only) + notification settings.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

use crate::domain::account::{Account, MAX_ACCOUNTS};
use crate::domain::window::WindowKind;
use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    /// Advance-warning minutes per window kind (default 15).
    pub reset_advance_minutes: HashMap<WindowKind, u32>,
    pub notify_on_free: bool,
    pub usage_threshold_percent: f32,
    pub cooldown_minutes: u32,
    /// (start, end) local time during which notifications are suppressed.
    pub quiet_hours: Option<(NaiveTime, NaiveTime)>,
    pub per_window_enabled: HashMap<WindowKind, bool>,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        let mut advance = HashMap::new();
        advance.insert(WindowKind::FiveHour, 15);
        advance.insert(WindowKind::Weekly, 15);
        advance.insert(WindowKind::WeeklySonnet, 15);

        let mut enabled = HashMap::new();
        enabled.insert(WindowKind::FiveHour, true);
        enabled.insert(WindowKind::Weekly, true);
        enabled.insert(WindowKind::WeeklySonnet, true);

        Self {
            reset_advance_minutes: advance,
            notify_on_free: true,
            usage_threshold_percent: 80.0,
            cooldown_minutes: 30,
            quiet_hours: None,
            per_window_enabled: enabled,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeMode {
    NextReset,
    AvailableCount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub notifications: NotificationSettings,
    #[serde(default = "default_interval")]
    pub polling_interval_secs: u64,
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(default = "default_badge")]
    pub badge_mode: BadgeMode,
}

fn default_interval() -> u64 {
    300
}
fn default_badge() -> BadgeMode {
    BadgeMode::NextReset
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            accounts: Vec::new(),
            notifications: NotificationSettings::default(),
            polling_interval_secs: default_interval(),
            launch_at_login: false,
            badge_mode: BadgeMode::NextReset,
        }
    }
}

impl AppConfig {
    pub fn add_account(&mut self, account: Account) -> Result<(), AppError> {
        if self.accounts.len() >= MAX_ACCOUNTS {
            return Err(AppError::AccountLimit(MAX_ACCOUNTS));
        }
        self.accounts.push(account);
        Ok(())
    }

    pub fn remove_account(&mut self, id: uuid::Uuid) {
        self.accounts.retain(|a| a.id != id);
    }
}

/// Loads/saves config as JSON in the platform config directory.
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            path: config_dir.join("config.json"),
        }
    }

    pub fn load(&self) -> AppConfig {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => AppConfig::default(),
        }
    }

    pub fn save(&self, config: &AppConfig) -> Result<(), AppError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Storage(e.to_string()))?;
        }
        let json = serde_json::to_string_pretty(config)
            .map_err(|e| AppError::Storage(e.to_string()))?;
        std::fs::write(&self.path, json).map_err(|e| AppError::Storage(e.to_string()))?;
        // Best-effort tighten permissions (unix).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}
