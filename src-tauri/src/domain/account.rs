//! Account model (up to 4 monitored accounts).

use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::window::Window;

/// Maximum number of accounts the app monitors.
pub const MAX_ACCOUNTS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    Pro,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Browser {
    Chrome,
    Brave,
    Firefox,
    Safari,
}

/// High-level availability of an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    /// Usable right now.
    Available,
    /// A 5-hour window is being consumed but not exhausted.
    InWindow,
    /// A limit is reached; blocked until reset.
    Limited,
}

/// Cross-cutting fetch state (orthogonal to `AccountStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FetchStatus {
    Ok,
    /// Serving previous values; last fetch failed transiently.
    Stale,
    /// Cookie missing/expired — user must re-login in the browser.
    AuthRequired,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub label: String,
    pub plan: Plan,
    pub browser: Browser,
    /// Identifier of the browser profile whose cookie store we read.
    pub browser_profile: String,
    /// IANA timezone name, e.g. "Asia/Tokyo".
    pub timezone: String,
    /// 0 = Monday .. 6 = Sunday (weekly reset day).
    pub weekly_reset_weekday: u8,
    pub weekly_reset_time: NaiveTime,

    #[serde(default)]
    pub windows: Vec<Window>,
    #[serde(default = "default_status")]
    pub status: AccountStatus,
    #[serde(default = "default_fetch")]
    pub fetch_status: FetchStatus,
    #[serde(default)]
    pub last_fetched_at: Option<DateTime<Utc>>,
    /// Start of the current 5-hour window (for fallback computation).
    #[serde(default)]
    pub window_started_at: Option<DateTime<Utc>>,
}

fn default_status() -> AccountStatus {
    AccountStatus::Available
}
fn default_fetch() -> FetchStatus {
    FetchStatus::Ok
}

impl Account {
    pub fn new(
        label: impl Into<String>,
        plan: Plan,
        browser: Browser,
        browser_profile: impl Into<String>,
        timezone: impl Into<String>,
        weekly_reset_weekday: u8,
        weekly_reset_time: NaiveTime,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.into(),
            plan,
            browser,
            browser_profile: browser_profile.into(),
            timezone: timezone.into(),
            weekly_reset_weekday: weekly_reset_weekday.min(6),
            weekly_reset_time,
            windows: Vec::new(),
            status: AccountStatus::Available,
            fetch_status: FetchStatus::Ok,
            last_fetched_at: None,
            window_started_at: None,
        }
    }

    /// Expected number of windows for the plan (Pro: 2, Max: 3).
    pub fn expected_window_count(&self) -> usize {
        match self.plan {
            Plan::Pro => 2,
            Plan::Max => 3,
        }
    }
}
