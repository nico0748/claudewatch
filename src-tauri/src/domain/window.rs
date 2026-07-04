//! Usage windows (5-hour rolling / weekly) and fetched snapshots.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Which limit a window represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowKind {
    /// Rolling 5-hour session window (starts at first prompt).
    FiveHour,
    /// Weekly limit across all models.
    Weekly,
    /// Weekly limit scoped to Sonnet models (Max plans only).
    WeeklySonnet,
}

impl WindowKind {
    pub fn label(&self) -> &'static str {
        match self {
            WindowKind::FiveHour => "5時間枠",
            WindowKind::Weekly => "週次枠",
            WindowKind::WeeklySonnet => "週次枠(Sonnet)",
        }
    }
}

/// Where a reset time came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Read directly from the Claude endpoint.
    Fetched,
    /// Computed locally (fallback when fetch is unavailable).
    Computed,
}

/// A single usage window for an account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    pub kind: WindowKind,
    /// 0.0 – 100.0
    pub usage_percent: f32,
    pub resets_at: DateTime<Utc>,
    pub resets_at_source: Source,
}

impl Window {
    pub fn is_exhausted(&self) -> bool {
        self.usage_percent >= 100.0
    }

    pub fn seconds_until_reset(&self, now: DateTime<Utc>) -> i64 {
        (self.resets_at - now).num_seconds()
    }
}

/// A point-in-time reading returned by the Claude API client.
#[derive(Debug, Clone)]
pub struct UsageSnapshot {
    pub windows: Vec<Window>,
    pub fetched_at: DateTime<Utc>,
}
