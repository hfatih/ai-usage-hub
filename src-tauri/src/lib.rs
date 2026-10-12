pub mod antigravity;
mod commands;
mod database;
mod detector;
mod error;
mod i18n;
mod keychain;
mod models;
pub mod proxy;
pub mod platform;
mod providers;
mod secret;

use std::{
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, RwLock,
    },
    thread,
    time::{Duration, Instant},
};

use chrono::Utc;
use database::Database;
use detector::ProcessDetector;
use error::{HubError, Result};
use models::{AppStatus, DashboardSnapshot, ProviderSummary};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_notification::NotificationExt;

pub struct HubState {
    pub(crate) database: Mutex<Database>,
    pub(crate) detector: Mutex<ProcessDetector>,
    pub(crate) statuses: RwLock<Vec<AppStatus>>,
    pub(crate) providers: RwLock<Vec<ProviderSummary>>,
    pub(crate) last_provider_refresh: Mutex<Option<Instant>>,
    pub(crate) provider_refreshing: AtomicBool,
    pub(crate) paused: AtomicBool,
    pub(crate) quitting: AtomicBool,
}

impl HubState {
    fn new(database: Database) -> Self {
        Self {
            database: Mutex::new(database),
            detector: Mutex::new(ProcessDetector::new()),
            statuses: RwLock::new(Vec::new()),
            providers: RwLock::new(providers::summaries()),
            last_provider_refresh: Mutex::new(None),
            provider_refreshing: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            quitting: AtomicBool::new(false),
        }
    }

    fn snapshot(&self) -> Result<DashboardSnapshot> {
        let applications = self
            .statuses
            .read()
            .map_err(|_| HubError::Operation("status lock is unavailable".into()))?
            .clone();
        let providers = self
            .providers
            .read()
            .map_err(|_| HubError::Operation("provider lock is unavailable".into()))?
            .clone();
        Ok(DashboardSnapshot {
            providers,
            applications,
            refreshed_at: Utc::now().timestamp(),
            monitoring_paused: self.paused.load(Ordering::Relaxed),
        })
    }
}

fn refresh_providers(state: &HubState, force: bool) -> Result<bool> {
    let due = force
        || state
            .last_provider_refresh
            .lock()
            .map_err(|_| HubError::Operation("provider refresh lock is unavailable".into()))?
            .is_none_or(|last| last.elapsed() >= Duration::from_secs(60));
    if !due || state.provider_refreshing.swap(true, Ordering::AcqRel) {
        return Ok(false);
    }

    let refresh_result = (|| {
        let previous = state
            .providers
            .read()
            .map_err(|_| HubError::Operation("provider lock is unavailable".into()))?
            .clone();
        let refreshed = providers::refresh_summaries(&previous);
        for provider in refreshed.iter().filter(|provider| {
            matches!(
                provider.id.as_str(),
                "openai" | "anthropic" | "google" | "opencode"
            )
        }) {
            if provider.error.is_none() {
                if let Ok(mut database) = state.database.lock() {
                    if let Err(error) = database.save_provider_snapshot(provider) {
                        tracing::warn!(%error, "could not persist provider usage snapshot");
                    }
                }
                // Outside the lock: the sync talks to the language server.
                if provider.id == "google" {
                    let _ = antigravity::sync_antigravity_active_session(&state.database);
                }
            } else if let Ok(database) = state.database.lock() {
                if let Err(error) = database.record_provider_error(provider) {
                    tracing::warn!(%error, "could not persist provider error notification");
                }
            }
        }
        *state
            .providers
            .write()
            .map_err(|_| HubError::Operation("provider lock is unavailable".into()))? = refreshed;
        *state
            .last_provider_refresh
            .lock()
            .map_err(|_| HubError::Operation("provider refresh lock is unavailable".into()))? =
            Some(Instant::now());
        Ok(true)
    })();
    state.provider_refreshing.store(false, Ordering::Release);
    refresh_result
}

fn refresh_state(state: &HubState) -> Result<(DashboardSnapshot, bool)> {
    if state.paused.load(Ordering::Relaxed) {
        return state.snapshot().map(|snapshot| (snapshot, false));
    }
    let applications = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .applications()?;
    let mut statuses = state
        .detector
        .lock()
        .map_err(|_| HubError::Operation("process detector lock is unavailable".into()))?
        .scan(&applications);
    let changed = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .sync_sessions(&mut statuses, Utc::now().timestamp())?;
    *state
        .statuses
        .write()
        .map_err(|_| HubError::Operation("status lock is unavailable".into()))? = statuses;
    state.snapshot().map(|snapshot| (snapshot, changed))
}

fn refresh_all(state: &HubState, force_provider: bool) -> Result<(DashboardSnapshot, bool)> {
    let (_, process_changed) = refresh_state(state)?;
    let provider_changed = if state.paused.load(Ordering::Relaxed) && !force_provider {
        false
    } else {
        refresh_providers(state, force_provider)?
    };
    state
        .snapshot()
        .map(|snapshot| (snapshot, process_changed || provider_changed))
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn tray_menu(app: &tauri::AppHandle, language: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let open = MenuItem::with_id(app, "open", i18n::text("open", language), true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", i18n::text("refresh", language), true, None::<&str>)?;
    let pause = MenuItem::with_id(
        app,
        "pause",
        i18n::text("pause", language),
        true,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, "settings", i18n::text("settings", language), true, None::<&str>)?;
    let codex = MenuItem::with_id(app, "launch:codex", i18n::text("codex", language), true, None::<&str>)?;
    let claude = MenuItem::with_id(app, "launch:claude", i18n::text("claude", language), true, None::<&str>)?;
    let gemini = MenuItem::with_id(app, "launch:gemini", i18n::text("gemini", language), true, None::<&str>)?;
    let antigravity = MenuItem::with_id(
        app,
        "launch:antigravity",
        i18n::text("antigravity", language),
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let separator_two = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", i18n::text("quit", language), true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &open,
            &refresh,
            &pause,
            &separator,
            &codex,
            &claude,
            &gemini,
            &antigravity,
            &separator_two,
            &settings,
            &quit,
        ],
    )
}

pub(crate) fn update_tray_language(app: &tauri::AppHandle, language: &str) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id("main-tray") {
        tray.set_menu(Some(tray_menu(app, language)?))?;
        tray.set_tooltip(Some(format!("AI Usage Hub\n{}", i18n::text("tooltip", language))))?;
    }
    Ok(())
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let language = app.state::<HubState>().database.lock().ok()
        .and_then(|db| db.settings().ok())
        .map(|settings| settings.language)
        .unwrap_or_else(|| "en".into());
    let menu = tray_menu(app.handle(), &language)?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip(format!("AI Usage Hub\n{}", i18n::text("tooltip", &language)))
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "settings" => {
                show_main(app);
                let _ = app.emit("hub://navigate", "settings");
            }
            "refresh" => {
                let state = app.state::<HubState>();
                if refresh_all(&state, true).is_ok() {
                    let _ = app.emit("hub://status-changed", ());
                }
            }
            "pause" => {
                let state = app.state::<HubState>();
                let paused = !state.paused.load(Ordering::Relaxed);
                state.paused.store(paused, Ordering::Relaxed);
                let _ = app.emit("hub://status-changed", ());
            }
            "quit" => {
                app.state::<HubState>()
                    .quitting
                    .store(true, Ordering::Relaxed);
                app.exit(0);
            }
            id if id.starts_with("launch:") => {
                let state = app.state::<HubState>();
                if let Err(error) = commands::launch(&state, &id[7..]) {
                    tracing::warn!(%error, "tray launch failed");
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn latest_alert_id(state: &HubState) -> i64 {
    state
        .database
        .lock()
        .ok()
        .and_then(|db| db.notifications(1).ok())
        .and_then(|alerts| alerts.first().map(|alert| alert.id))
        .unwrap_or(0)
}

/// Raises a desktop notification for each usage alert recorded since `before` (at most three per pass),
/// so a low quota is noticed while the window is hidden in the tray.
fn show_new_alerts(app: &tauri::AppHandle, state: &HubState, before: i64) {
    let Ok(db) = state.database.lock() else { return };
    let Ok(settings) = db.settings() else { return };
    if !settings.desktop_notifications || !settings.notifications_enabled {
        return;
    }
    let alerts = db.notifications(3).unwrap_or_default();
    drop(db);
    for alert in alerts.iter().rev().filter(|alert| alert.id > before) {
        let (title, body) = i18n::alert_text(&alert.title, alert.detail.as_deref(), &settings.language);
        if let Err(error) = app.notification().builder().title(title).body(body).show() {
            tracing::warn!(%error, "could not show desktop notification");
        }
    }
}

fn start_monitor(app: tauri::AppHandle) {
    thread::spawn(move || {
      let mut last_pool_refresh: Option<Instant> = None;
      loop {
        let state = app.state::<HubState>();
        // Pull real quota for every pooled account from Google every ten minutes, so the
        // accounts that are not open in the IDE do not sit on stale numbers.
        if last_pool_refresh.is_none_or(|last| last.elapsed() >= Duration::from_secs(600)) {
            last_pool_refresh = Some(Instant::now());
            if !state.paused.load(Ordering::Relaxed) {
                let _ = antigravity::refresh_all_accounts(&state.database);
            }
        }
        let alerts_before = latest_alert_id(&state);
        match refresh_all(&state, false) {
            Ok((snapshot, changed)) => {
                show_new_alerts(&app, &state, alerts_before);
                let running = snapshot
                    .applications
                    .iter()
                    .filter(|item| matches!(item.state, models::RunningState::Running))
                    .count();
                if let Some(tray) = app.tray_by_id("main-tray") {
                    let language = state.database.lock().ok()
                        .and_then(|db| db.settings().ok())
                        .map(|settings| settings.language)
                        .unwrap_or_else(|| "en".into());
                    let _ = tray.set_tooltip(Some(i18n::running_tooltip(running, &language)));
                }
                if changed {
                    let _ = app.emit("hub://status-changed", ());
                }
            }
            Err(error) => tracing::warn!(%error, "background refresh failed"),
        }
        if let Ok(Some(switched)) = antigravity::check_and_auto_switch(&app.state::<HubState>()) {
            tracing::info!(account = %switched.email, "Antigravity auto-switched account");
            let _ = app.emit("hub://status-changed", ());
            let _ = app.emit("hub://antigravity-switched", &switched);
        }
        let interval = app
            .state::<HubState>()
            .database
            .lock()
            .ok()
            .and_then(|db| db.settings().ok())
            .map(|settings| settings.refresh_interval_seconds)
            .unwrap_or(5)
            .clamp(3, 300);
        thread::sleep(Duration::from_secs(interval));
      }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Before any thread starts: a Finder-launched macOS app has no Homebrew/npm folders on PATH.
    platform::extend_search_path();

    let log_file = platform::log_dir().and_then(|dir| {
        let _ = fs::create_dir_all(&dir);
        fs::OpenOptions::new().create(true).append(true).open(dir.join("app.log")).ok()
    });

    if let Some(file) = log_file {
        let _ = tracing_subscriber::fmt()
            .with_writer(file)
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "ai_usage_hub=info,warn".into()),
            )
            .with_target(false)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "ai_usage_hub=info,warn".into()),
            )
            .with_target(false)
            .try_init();
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            tracing::info!("Starting setup...");
            let app_data = match app.path().app_data_dir() {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to get app_data_dir");
                    return Err(e.into());
                }
            };
            let _ = fs::create_dir_all(&app_data);
            let database = match Database::open(&app_data.join("ai-usage-hub.sqlite3")) {
                Ok(db) => db,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to open sqlite database");
                    return Err(e.into());
                }
            };
            app.manage(HubState::new(database));
            let state = app.state::<HubState>();
            if let Ok(db) = state.database.lock() {
                if let Ok(mut settings) = db.settings() {
                    if settings.proxy_api_key.is_empty() {
                        settings.proxy_api_key = proxy::generate_api_key();
                        let _ = db.save_settings(&settings);
                    }
                    proxy::set_model_map(settings.proxy_model_map);
                }
            }
            if let Err(e) = refresh_state(&state) {
                tracing::warn!(error = %e, "refresh_state failed during setup");
            }
            if let Err(e) = build_tray(app) {
                tracing::error!(error = %e, "build_tray failed during setup");
                return Err(e.into());
            }
            if std::env::args().any(|arg| arg == "--minimized") {
                if let Some(window) = app.get_webview_window("main") {
                    window.hide()?;
                }
            }
            start_monitor(app.handle().clone());
            proxy::start_proxy_server(app.handle().clone(), proxy::DEFAULT_PROXY_PORT);
            tracing::info!("Setup completed successfully!");
            Ok(())
        })
        .on_window_event(|window, event| {
            tracing::info!("Window event: {:?}", event);
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<HubState>();
                if state.quitting.load(Ordering::Relaxed) {
                    return;
                }
                let close_to_tray = state
                    .database
                    .lock()
                    .ok()
                    .and_then(|db| db.settings().ok())
                    .is_none_or(|settings| settings.close_behavior == "tray");
                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_dashboard,
            commands::refresh_processes,
            commands::get_activity,
            commands::get_accounts,
            commands::delete_account,
            commands::get_usage_history,
            commands::get_notifications,
            commands::get_security_status,
            commands::clear_usage_history,
            commands::get_settings,
            commands::update_settings,
            commands::set_monitoring_paused,
            commands::set_app_executable,
            commands::launch_app,
            commands::get_antigravity_accounts,
            commands::capture_current_antigravity_account,
            commands::add_antigravity_account,
            commands::delete_antigravity_account,
            commands::switch_antigravity_account,
            commands::toggle_antigravity_auto_switch,
            commands::get_antigravity_oauth_url,
            commands::start_antigravity_oauth_login,
            commands::complete_antigravity_oauth_code,
            commands::export_antigravity_accounts,
            commands::import_antigravity_accounts,
            commands::refresh_all_antigravity_accounts,
            commands::scan_local_antigravity_accounts,
            commands::get_antigravity_live_session,
            commands::sync_antigravity_session,
            commands::get_proxy_status,
            commands::test_proxy_prompt,
            commands::get_proxy_models,
            commands::regenerate_proxy_api_key,
            commands::clear_proxy_logs,
        ])
        .build(tauri::generate_context!())
        .expect("AI Usage Hub failed to start")
        .run(|_app, _event| {
            // Clicking the Dock icon while the window is hidden in the menu bar brings it back.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { has_visible_windows: false, .. } = _event {
                show_main(_app);
            }
        });
}
