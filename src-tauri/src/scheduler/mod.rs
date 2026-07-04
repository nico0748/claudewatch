//! Polling scheduler with jitter and per-account exponential backoff.

use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::app::{refresh_all, AppState};

/// Spawn the background polling loop. Runs until the app exits.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Do an immediate first pass so the UI isn't empty on launch.
        refresh_all(&app).await;

        loop {
            let interval = {
                let state = app.state::<AppState>();
                let cfg = state.config.lock().unwrap();
                cfg.polling_interval_secs.max(60)
            };
            let jitter = jitter_secs();
            tokio::time::sleep(Duration::from_secs(interval + jitter)).await;
            refresh_all(&app).await;
        }
    });
}

/// Small random jitter (0–20s) to avoid perfectly periodic requests.
fn jitter_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 21) as u64
}
