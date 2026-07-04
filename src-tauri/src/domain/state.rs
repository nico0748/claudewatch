//! State engine: reset-time computation, status evaluation, transition detection.

use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use chrono_tz::Tz;

use super::account::{Account, AccountStatus, FetchStatus};
use super::window::{Source, UsageSnapshot, Window, WindowKind};

/// Length of the rolling session window.
const FIVE_HOUR: Duration = Duration::hours(5);

/// A detected change in account/fetch state, used to trigger notifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    Status { from: AccountStatus, to: AccountStatus },
    Fetch { from: FetchStatus, to: FetchStatus },
}

/// Apply a freshly fetched snapshot to the account, returning transitions.
///
/// Fetched reset times always take precedence; missing windows are filled by
/// local computation so the UI can keep counting down even if a field is absent.
pub fn apply_snapshot(account: &mut Account, snap: UsageSnapshot) -> Vec<Transition> {
    let prev_status = account.status;
    let prev_fetch = account.fetch_status;

    account.windows = merge_windows(account, snap.windows, snap.fetched_at);
    account.last_fetched_at = Some(snap.fetched_at);
    account.fetch_status = FetchStatus::Ok;
    account.status = evaluate_status(&account.windows);

    // Track the 5-hour window start for future fallback computation.
    if let Some(w) = account.windows.iter().find(|w| w.kind == WindowKind::FiveHour) {
        if w.usage_percent > 0.0 && account.window_started_at.is_none() {
            account.window_started_at = Some(w.resets_at - FIVE_HOUR);
        }
        if w.usage_percent <= 0.0 {
            account.window_started_at = None;
        }
    }

    collect_transitions(prev_status, account.status, prev_fetch, account.fetch_status)
}

/// Mark the account as failing to fetch without discarding previous values.
pub fn mark_fetch_failure(account: &mut Account, status: FetchStatus) -> Vec<Transition> {
    let prev = account.fetch_status;
    account.fetch_status = status;
    collect_transitions(account.status, account.status, prev, status)
}

/// Recompute reset-driven transitions on a tick (e.g. a window resetting).
pub fn tick_reset(account: &mut Account, now: DateTime<Utc>) -> Vec<Transition> {
    let prev_status = account.status;
    let mut changed = false;
    for w in account.windows.iter_mut() {
        if w.resets_at <= now {
            w.usage_percent = 0.0;
            changed = true;
        }
    }
    if changed {
        account.window_started_at = None;
        account.status = evaluate_status(&account.windows);
    }
    collect_transitions(prev_status, account.status, account.fetch_status, account.fetch_status)
}

fn merge_windows(
    account: &Account,
    fetched: Vec<Window>,
    now: DateTime<Utc>,
) -> Vec<Window> {
    let mut out = fetched;
    // Ensure the weekly window exists; compute it if the endpoint didn't return it.
    let has_weekly = out.iter().any(|w| w.kind == WindowKind::Weekly);
    if !has_weekly {
        out.push(Window {
            kind: WindowKind::Weekly,
            usage_percent: 0.0,
            resets_at: next_weekly_reset(account, now),
            resets_at_source: Source::Computed,
        });
    }
    out
}

/// Determine account availability from its windows.
pub fn evaluate_status(windows: &[Window]) -> AccountStatus {
    if windows.iter().any(|w| w.is_exhausted()) {
        return AccountStatus::Limited;
    }
    let in_window = windows
        .iter()
        .any(|w| w.kind == WindowKind::FiveHour && w.usage_percent > 0.0);
    if in_window {
        AccountStatus::InWindow
    } else {
        AccountStatus::Available
    }
}

/// Next occurrence of the account's fixed weekly reset, in UTC.
pub fn next_weekly_reset(account: &Account, now: DateTime<Utc>) -> DateTime<Utc> {
    let tz: Tz = account.timezone.parse().unwrap_or(chrono_tz::UTC);
    let now_local = now.with_timezone(&tz);

    // Target weekday: our model uses 0=Mon..6=Sun; chrono uses Mon=0 via weekday().num_days_from_monday().
    let target = account.weekly_reset_weekday as i64;
    let today = now_local.weekday().num_days_from_monday() as i64;
    let mut days_ahead = (target - today).rem_euclid(7);

    let candidate_date = (now_local + Duration::days(days_ahead)).date_naive();
    let naive_dt = candidate_date.and_time(account.weekly_reset_time);

    let mut local_dt = match tz.from_local_datetime(&naive_dt).single() {
        Some(dt) => dt,
        // DST gap/fold: nudge forward and retry.
        None => tz
            .from_local_datetime(&(naive_dt + Duration::hours(1)))
            .earliest()
            .unwrap_or_else(|| tz.from_utc_datetime(&naive_dt)),
    };

    // If the computed time is already in the past today, roll a week forward.
    if local_dt.with_timezone(&Utc) <= now {
        days_ahead += 7;
        let d = (now_local + Duration::days(days_ahead)).date_naive();
        let ndt = d.and_time(account.weekly_reset_time);
        local_dt = tz
            .from_local_datetime(&ndt)
            .single()
            .unwrap_or_else(|| tz.from_utc_datetime(&ndt));
    }

    local_dt.with_timezone(&Utc)
}

fn collect_transitions(
    ps: AccountStatus,
    ns: AccountStatus,
    pf: FetchStatus,
    nf: FetchStatus,
) -> Vec<Transition> {
    let mut v = Vec::new();
    if ps != ns {
        v.push(Transition::Status { from: ps, to: ns });
    }
    if pf != nf {
        v.push(Transition::Fetch { from: pf, to: nf });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::account::{Browser, Plan};
    use chrono::NaiveTime;

    fn sample_account() -> Account {
        Account::new(
            "test",
            Plan::Max,
            Browser::Firefox,
            "default",
            "Asia/Tokyo",
            0, // Monday
            NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
        )
    }

    #[test]
    fn weekly_reset_is_in_future() {
        let acc = sample_account();
        let now = Utc::now();
        assert!(next_weekly_reset(&acc, now) > now);
    }

    #[test]
    fn status_limited_when_exhausted() {
        let w = vec![Window {
            kind: WindowKind::Weekly,
            usage_percent: 100.0,
            resets_at: Utc::now(),
            resets_at_source: Source::Fetched,
        }];
        assert_eq!(evaluate_status(&w), AccountStatus::Limited);
    }

    #[test]
    fn status_available_when_idle() {
        let w = vec![Window {
            kind: WindowKind::FiveHour,
            usage_percent: 0.0,
            resets_at: Utc::now(),
            resets_at_source: Source::Fetched,
        }];
        assert_eq!(evaluate_status(&w), AccountStatus::Available);
    }
}
