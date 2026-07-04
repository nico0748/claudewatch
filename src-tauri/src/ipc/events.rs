//! Event names and view DTOs sent to the frontend.
//!
//! DTOs never carry cookie values or other secrets.

use chrono::Utc;
use serde::Serialize;

use crate::domain::account::{Account, AccountStatus, Browser, FetchStatus, Plan};
use crate::domain::window::WindowKind;

pub const ACCOUNT_UPDATED: &str = "account_updated";
pub const FETCH_ERROR: &str = "fetch_error";
pub const AUTH_REQUIRED: &str = "auth_required";
pub const NOTIFICATION_SENT: &str = "notification_sent";

#[derive(Debug, Clone, Serialize)]
pub struct WindowView {
    pub kind: WindowKind,
    pub label: String,
    pub usage_percent: f32,
    pub resets_at: String,          // RFC3339
    pub seconds_until_reset: i64,
    pub computed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountView {
    pub id: String,
    pub label: String,
    pub plan: Plan,
    pub browser: Browser,
    pub status: AccountStatus,
    pub fetch_status: FetchStatus,
    pub windows: Vec<WindowView>,
    pub last_fetched_at: Option<String>,
}

impl From<&Account> for AccountView {
    fn from(a: &Account) -> Self {
        let now = Utc::now();
        let windows = a
            .windows
            .iter()
            .map(|w| WindowView {
                kind: w.kind,
                label: w.kind.label().to_string(),
                usage_percent: w.usage_percent,
                resets_at: w.resets_at.to_rfc3339(),
                seconds_until_reset: w.seconds_until_reset(now),
                computed: matches!(w.resets_at_source, crate::domain::window::Source::Computed),
            })
            .collect();

        AccountView {
            id: a.id.to_string(),
            label: a.label.clone(),
            plan: a.plan,
            browser: a.browser,
            status: a.status,
            fetch_status: a.fetch_status,
            windows,
            last_fetched_at: a.last_fetched_at.map(|t| t.to_rfc3339()),
        }
    }
}
