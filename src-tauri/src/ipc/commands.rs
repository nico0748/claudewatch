//! Tauri commands invoked from the frontend.

use chrono::NaiveTime;
use serde::Deserialize;
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::app::{refresh_all, AppState};
use crate::acquisition::cookie::provider_for;
use crate::domain::account::{Account, Browser, Plan};
use crate::error::{AppError, Result};
use crate::ipc::events::AccountView;
use crate::storage::config::NotificationSettings;

#[derive(Debug, Deserialize)]
pub struct NewAccount {
    pub label: String,
    pub plan: Plan,
    pub browser: Browser,
    pub browser_profile: String,
    pub timezone: String,
    pub weekly_reset_weekday: u8,
    /// "HH:MM" local time.
    pub weekly_reset_time: String,
}

#[derive(Debug, Deserialize)]
pub struct AccountPatch {
    pub id: String,
    pub label: Option<String>,
    pub browser: Option<Browser>,
    pub browser_profile: Option<String>,
    pub timezone: Option<String>,
    pub weekly_reset_weekday: Option<u8>,
    pub weekly_reset_time: Option<String>,
}

fn parse_time(s: &str) -> NaiveTime {
    NaiveTime::parse_from_str(s, "%H:%M").unwrap_or_else(|_| NaiveTime::from_hms_opt(9, 0, 0).unwrap())
}

#[tauri::command]
pub fn get_accounts(state: State<'_, AppState>) -> Vec<AccountView> {
    let cfg = state.config.lock().unwrap();
    cfg.accounts.iter().map(AccountView::from).collect()
}

#[tauri::command]
pub fn add_account(state: State<'_, AppState>, account: NewAccount) -> Result<AccountView> {
    let acc = Account::new(
        account.label,
        account.plan,
        account.browser,
        account.browser_profile,
        account.timezone,
        account.weekly_reset_weekday,
        parse_time(&account.weekly_reset_time),
    );
    let view = AccountView::from(&acc);
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.add_account(acc)?;
    }
    state.persist();
    Ok(view)
}

#[tauri::command]
pub fn update_account(state: State<'_, AppState>, patch: AccountPatch) -> Result<AccountView> {
    let id = Uuid::parse_str(&patch.id).map_err(|e| AppError::Storage(e.to_string()))?;
    let mut cfg = state.config.lock().unwrap();
    let acc = cfg
        .accounts
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| AppError::Storage("account not found".into()))?;

    if let Some(v) = patch.label { acc.label = v; }
    if let Some(v) = patch.browser { acc.browser = v; }
    if let Some(v) = patch.browser_profile { acc.browser_profile = v; }
    if let Some(v) = patch.timezone { acc.timezone = v; }
    if let Some(v) = patch.weekly_reset_weekday { acc.weekly_reset_weekday = v.min(6); }
    if let Some(v) = patch.weekly_reset_time { acc.weekly_reset_time = parse_time(&v); }

    let view = AccountView::from(&*acc);
    drop(cfg);
    state.persist();
    Ok(view)
}

#[tauri::command]
pub fn remove_account(state: State<'_, AppState>, id: String) -> Result<()> {
    let id = Uuid::parse_str(&id).map_err(|e| AppError::Storage(e.to_string()))?;
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.remove_account(id);
    }
    state.persist();
    Ok(())
}

#[tauri::command]
pub fn detect_browser_profiles(browser: Browser) -> Result<Vec<crate::acquisition::cookie::ProfileInfo>> {
    let provider = provider_for(browser);
    provider.detect_profiles().map_err(AppError::from)
}

#[tauri::command]
pub async fn refresh_now(app: AppHandle) -> Result<()> {
    refresh_all(&app).await;
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> NotificationSettings {
    state.config.lock().unwrap().notifications.clone()
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, settings: NotificationSettings) -> Result<()> {
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.notifications = settings.clone();
    }
    {
        let mut notifier = state.notifier.lock().unwrap();
        notifier.update_settings(settings);
    }
    state.persist();
    Ok(())
}
