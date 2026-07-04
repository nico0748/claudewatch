//! Shared application state and the per-account refresh routine.

use std::sync::Mutex;

use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::acquisition::cookie::provider_for;
use crate::acquisition::ClaudeClient;
use crate::domain::account::FetchStatus;
use crate::domain::state::{apply_snapshot, mark_fetch_failure};
use crate::error::{ApiError, CookieError};
use crate::ipc::events::{self, AccountView};
use crate::notification::NotificationManager;
use crate::storage::config::AppConfig;
use crate::storage::ConfigStore;

/// Tauri-managed global state.
pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub notifier: Mutex<NotificationManager>,
    pub client: ClaudeClient,
    pub store: ConfigStore,
}

impl AppState {
    pub fn new(store: ConfigStore) -> Self {
        let config = store.load();
        let notifier = NotificationManager::new(config.notifications.clone());
        Self {
            config: Mutex::new(config),
            notifier: Mutex::new(notifier),
            client: ClaudeClient::new(),
            store,
        }
    }

    pub fn persist(&self) {
        if let Ok(cfg) = self.config.lock() {
            let _ = self.store.save(&cfg);
        }
    }
}

/// Refresh every account once, emitting updates and firing notifications.
pub async fn refresh_all(app: &AppHandle) {
    let state = app.state::<AppState>();

    // Snapshot the account list (ids + browser refs) to avoid holding the lock
    // across await points.
    let accounts: Vec<_> = {
        let cfg = state.config.lock().unwrap();
        cfg.accounts.clone()
    };

    for account in accounts {
        let result = fetch_one(&state.client, &account).await;

        let (view, notices) = {
            let mut cfg = state.config.lock().unwrap();
            let Some(acc) = cfg.accounts.iter_mut().find(|a| a.id == account.id) else {
                continue;
            };

            let transitions = match result {
                Ok(snap) => apply_snapshot(acc, snap),
                Err(FetchOutcome::Auth) => mark_fetch_failure(acc, FetchStatus::AuthRequired),
                Err(FetchOutcome::Transient) => mark_fetch_failure(acc, FetchStatus::Stale),
                Err(FetchOutcome::Fatal) => mark_fetch_failure(acc, FetchStatus::Error),
            };

            let notices = {
                let mut notifier = state.notifier.lock().unwrap();
                notifier.evaluate(acc, &transitions, Utc::now())
            };
            (AccountView::from(&*acc), notices)
        };

        let _ = app.emit(events::ACCOUNT_UPDATED, &view);

        for notice in notices {
            let _ = app
                .notification()
                .builder()
                .title(notice.title())
                .body(notice.body())
                .show();
            let _ = app.emit(
                events::NOTIFICATION_SENT,
                serde_json::json!({ "account": view.label, "title": notice.title() }),
            );
        }
    }

    state.persist();
}

enum FetchOutcome {
    Auth,
    Transient,
    Fatal,
}

async fn fetch_one(
    client: &ClaudeClient,
    account: &crate::domain::account::Account,
) -> Result<crate::domain::window::UsageSnapshot, FetchOutcome> {
    let provider = provider_for(account.browser);
    let bundle = provider
        .read_claude_cookies(&account.browser_profile)
        .map_err(|e| match e {
            CookieError::CookieMissing
            | CookieError::DecryptKeyUnavailable
            | CookieError::PermissionDenied => FetchOutcome::Auth,
            CookieError::CookieStoreLocked => FetchOutcome::Transient,
            _ => FetchOutcome::Fatal,
        })?;

    client.fetch_usage(&bundle).await.map_err(|e| match e {
        ApiError::Unauthorized => FetchOutcome::Auth,
        ApiError::RateLimited | ApiError::Network(_) | ApiError::Server(_) => {
            FetchOutcome::Transient
        }
        ApiError::Parse(_) => FetchOutcome::Transient,
    })
}
