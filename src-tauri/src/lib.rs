//! claudewatch core library (Tauri entry point).

pub mod acquisition;
pub mod app;
pub mod domain;
pub mod error;
pub mod ipc;
pub mod notification;
pub mod scheduler;
pub mod storage;

use tauri::{Manager, WindowEvent};

use crate::app::AppState;
use crate::storage::ConfigStore;

/// Build and run the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "claudewatch=info".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .expect("could not resolve app config dir");
            let store = ConfigStore::new(config_dir);
            app.manage(AppState::new(store));

            // Start background polling (5-minute default).
            scheduler::spawn(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Keep the app resident: closing the popover just hides it.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            ipc::commands::get_accounts,
            ipc::commands::add_account,
            ipc::commands::update_account,
            ipc::commands::remove_account,
            ipc::commands::detect_browser_profiles,
            ipc::commands::refresh_now,
            ipc::commands::get_settings,
            ipc::commands::update_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running claudewatch");
}
