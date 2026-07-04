//! Client for Claude's internal usage endpoints.
//!
//! NOTE: The exact endpoint paths and response schema are provisional and must
//! be confirmed against the live service (see docs/lld.md §3.2). The parser is
//! deliberately tolerant of missing/renamed fields.

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::cookie::CookieBundle;
use crate::domain::window::{Source, UsageSnapshot, Window, WindowKind};
use crate::error::ApiError;

const BASE_URL: &str = "https://claude.ai";
const USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

pub struct ClaudeClient {
    http: reqwest::Client,
}

impl ClaudeClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("failed to build reqwest client");
        Self { http }
    }

    /// Fetch a usage snapshot using the given browser cookies.
    pub async fn fetch_usage(&self, cookies: &CookieBundle) -> Result<UsageSnapshot, ApiError> {
        let cookie_header = cookies.header_value();

        // Step 1: resolve the organization id.
        let orgs = self
            .get_json("/api/organizations", &cookie_header)
            .await?;
        let org_id = extract_org_id(&orgs)
            .ok_or_else(|| ApiError::Parse("no organization id".into()))?;

        // Step 2: fetch usage for that organization.
        // Endpoint path is provisional.
        let path = format!("/api/bootstrap/{org_id}/usage");
        let usage = self.get_json(&path, &cookie_header).await?;

        parse_usage(&usage, Utc::now())
    }

    async fn get_json(&self, path: &str, cookie: &str) -> Result<Value, ApiError> {
        let url = format!("{BASE_URL}{path}");
        let resp = self
            .http
            .get(&url)
            .header(reqwest::header::COOKIE, cookie)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        match resp.status().as_u16() {
            200 => resp
                .json::<Value>()
                .await
                .map_err(|e| ApiError::Parse(e.to_string())),
            401 | 403 => Err(ApiError::Unauthorized),
            429 => Err(ApiError::RateLimited),
            s if s >= 500 => Err(ApiError::Server(s)),
            s => Err(ApiError::Server(s)),
        }
    }
}

impl Default for ClaudeClient {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_org_id(v: &Value) -> Option<String> {
    v.as_array()?
        .iter()
        .find_map(|o| o.get("uuid").and_then(|u| u.as_str()).map(String::from))
}

/// Tolerant parser: maps whatever windows are present, skips the rest.
fn parse_usage(v: &Value, now: DateTime<Utc>) -> Result<UsageSnapshot, ApiError> {
    let mut windows = Vec::new();

    // Provisional shape: { "five_hour": {...}, "weekly": {...}, "weekly_sonnet": {...} }
    for (key, kind) in [
        ("five_hour", WindowKind::FiveHour),
        ("weekly", WindowKind::Weekly),
        ("weekly_sonnet", WindowKind::WeeklySonnet),
    ] {
        if let Some(obj) = v.get(key) {
            if let Some(w) = parse_window(obj, kind, now) {
                windows.push(w);
            }
        }
    }

    Ok(UsageSnapshot {
        windows,
        fetched_at: now,
    })
}

fn parse_window(obj: &Value, kind: WindowKind, now: DateTime<Utc>) -> Option<Window> {
    let usage_percent = obj
        .get("utilization")
        .or_else(|| obj.get("usage_percent"))
        .and_then(|x| x.as_f64())
        .map(|x| x as f32)
        .unwrap_or(0.0);

    let resets_at = obj
        .get("resets_at")
        .and_then(|x| x.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    Some(Window {
        kind,
        usage_percent: usage_percent.clamp(0.0, 100.0),
        resets_at: resets_at.unwrap_or(now),
        resets_at_source: if resets_at.is_some() {
            Source::Fetched
        } else {
            Source::Computed
        },
    })
}
