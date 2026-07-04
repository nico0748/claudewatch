//! Notification manager: decides what to notify and enforces suppression.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Local, NaiveTime, Utc};

use crate::domain::account::{Account, AccountStatus, FetchStatus};
use crate::domain::state::Transition;
use crate::domain::window::WindowKind;
use crate::storage::config::NotificationSettings;

/// A notification the app decided to raise.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    ResetAdvance { account: String, window: WindowKind, minutes: i64 },
    FreeAccount { account: String },
    UsageThreshold { account: String, window: WindowKind, percent: f32 },
    AuthRequired { account: String },
}

impl Notice {
    pub fn title(&self) -> String {
        match self {
            Notice::ResetAdvance { .. } => "リセット予告".into(),
            Notice::FreeAccount { .. } => "アカウントが使えます".into(),
            Notice::UsageThreshold { .. } => "上限に接近".into(),
            Notice::AuthRequired { .. } => "要再ログイン".into(),
        }
    }

    pub fn body(&self) -> String {
        match self {
            Notice::ResetAdvance { account, window, minutes } => {
                format!("「{account}」{}があと{minutes}分でリセット", window.label())
            }
            Notice::FreeAccount { account } => {
                format!("「{account}」が使えるようになりました")
            }
            Notice::UsageThreshold { account, window, percent } => {
                format!("「{account}」{}が{percent:.0}%に到達", window.label())
            }
            Notice::AuthRequired { account } => {
                format!("「{account}」はブラウザで再ログインが必要です")
            }
        }
    }

    /// Dedup/cooldown key.
    fn key(&self) -> String {
        match self {
            Notice::ResetAdvance { account, window, .. } => format!("adv:{account}:{window:?}"),
            Notice::FreeAccount { account } => format!("free:{account}"),
            Notice::UsageThreshold { account, window, .. } => format!("thr:{account}:{window:?}"),
            Notice::AuthRequired { account } => format!("auth:{account}"),
        }
    }
}

pub struct NotificationManager {
    settings: NotificationSettings,
    last_sent: HashMap<String, DateTime<Utc>>,
}

impl NotificationManager {
    pub fn new(settings: NotificationSettings) -> Self {
        Self {
            settings,
            last_sent: HashMap::new(),
        }
    }

    pub fn update_settings(&mut self, settings: NotificationSettings) {
        self.settings = settings;
    }

    /// Evaluate an account (after a refresh) and return notices to send.
    pub fn evaluate(
        &mut self,
        account: &Account,
        transitions: &[Transition],
        now: DateTime<Utc>,
    ) -> Vec<Notice> {
        let mut candidates = Vec::new();

        // 1. Free account (status -> Available).
        if self.settings.notify_on_free
            && transitions.iter().any(|t| {
                matches!(t, Transition::Status { to: AccountStatus::Available, .. })
            })
        {
            candidates.push(Notice::FreeAccount {
                account: account.label.clone(),
            });
        }

        // 2. Reset advance warnings (per window).
        for w in &account.windows {
            let enabled = *self
                .settings
                .per_window_enabled
                .get(&w.kind)
                .unwrap_or(&true);
            if !enabled {
                continue;
            }
            let advance = *self
                .settings
                .reset_advance_minutes
                .get(&w.kind)
                .unwrap_or(&15) as i64;
            let mins_left = w.seconds_until_reset(now) / 60;
            if mins_left >= 0 && mins_left <= advance {
                candidates.push(Notice::ResetAdvance {
                    account: account.label.clone(),
                    window: w.kind,
                    minutes: mins_left,
                });
            }
        }

        // 3. Usage threshold.
        for w in &account.windows {
            if w.usage_percent >= self.settings.usage_threshold_percent {
                candidates.push(Notice::UsageThreshold {
                    account: account.label.clone(),
                    window: w.kind,
                    percent: w.usage_percent,
                });
            }
        }

        // 4. Auth required.
        if transitions.iter().any(|t| {
            matches!(t, Transition::Fetch { to: FetchStatus::AuthRequired, .. })
        }) {
            candidates.push(Notice::AuthRequired {
                account: account.label.clone(),
            });
        }

        candidates
            .into_iter()
            .filter(|n| self.should_send(n, now))
            .collect()
    }

    fn should_send(&mut self, notice: &Notice, now: DateTime<Utc>) -> bool {
        if self.in_quiet_hours(now) {
            return false;
        }
        let key = notice.key();
        let cooldown = chrono::Duration::minutes(self.settings.cooldown_minutes as i64);
        if let Some(&last) = self.last_sent.get(&key) {
            if now - last < cooldown {
                return false;
            }
        }
        self.last_sent.insert(key, now);
        true
    }

    fn in_quiet_hours(&self, now: DateTime<Utc>) -> bool {
        let Some((start, end)) = self.settings.quiet_hours else {
            return false;
        };
        let local: NaiveTime = now.with_timezone(&Local).time();
        if start <= end {
            local >= start && local < end
        } else {
            // Overnight window (e.g. 22:00–07:00).
            local >= start || local < end
        }
    }

    /// Clear the per-reset "already notified" flags for a window that reset.
    pub fn clear_for_reset(&mut self, account_label: &str, window: WindowKind) {
        let adv = format!("adv:{account_label}:{window:?}");
        let thr = format!("thr:{account_label}:{window:?}");
        self.last_sent.remove(&adv);
        self.last_sent.remove(&thr);
    }
}

/// Helper for weekday-aware future work (kept for scheduler use).
pub fn is_same_local_day(a: DateTime<Utc>, b: DateTime<Utc>) -> bool {
    a.with_timezone(&Local).date_naive().day() == b.with_timezone(&Local).date_naive().day()
}
