use std::{path::Path, process::Command, sync::atomic::Ordering};

use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    detector::resolve_executable,
    error::{HubError, Result},
    models::{
        AccountSummary, ActivityEvent, AntigravityAccount, AntigravityLiveSession, AppSettings,
        DashboardSnapshot, SecurityStatus, UsageHistoryPoint,
    },
    refresh_all, update_tray_language, HubState,
};

#[tauri::command]
pub fn get_dashboard(state: State<'_, HubState>) -> Result<DashboardSnapshot> {
    state.snapshot()
}

#[tauri::command(async)]
pub fn refresh_processes(state: State<'_, HubState>) -> Result<DashboardSnapshot> {
    refresh_all(&state, true).map(|(snapshot, _)| snapshot)
}

#[tauri::command]
pub fn get_activity(state: State<'_, HubState>) -> Result<Vec<ActivityEvent>> {
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .activity(100)
}

#[tauri::command]
pub fn get_accounts(state: State<'_, HubState>) -> Result<Vec<AccountSummary>> {
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .accounts()
}

#[tauri::command]
pub fn delete_account(state: State<'_, HubState>, account_id: String) -> Result<bool> {
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .delete_account(&account_id)
}

#[tauri::command]
pub fn get_usage_history(state: State<'_, HubState>) -> Result<Vec<UsageHistoryPoint>> {
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .usage_history(250)
}

#[tauri::command]
pub fn get_notifications(state: State<'_, HubState>) -> Result<Vec<ActivityEvent>> {
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .notifications(100)
}

#[tauri::command]
pub fn get_security_status(state: State<'_, HubState>) -> Result<SecurityStatus> {
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .security_status()
}

#[tauri::command]
pub fn clear_usage_history(state: State<'_, HubState>) -> Result<usize> {
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .clear_usage_history()
}

#[tauri::command]
pub fn get_settings(state: State<'_, HubState>) -> Result<AppSettings> {
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .settings()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, HubState>,
    mut settings: AppSettings,
) -> Result<AppSettings> {
    settings.refresh_interval_seconds = settings.refresh_interval_seconds.clamp(3, 300);
    settings.notify_warning_percent = settings.notify_warning_percent.clamp(1, 99);
    settings.notify_critical_percent = settings
        .notify_critical_percent
        .clamp(1, settings.notify_warning_percent);
    if !matches!(settings.close_behavior.as_str(), "tray" | "quit") {
        return Err(HubError::Configuration(
            "close behavior must be 'tray' or 'quit'".into(),
        ));
    }
    if !matches!(settings.theme.as_str(), "dark" | "light" | "system") {
        return Err(HubError::Configuration("unsupported theme".into()));
    }
    if !matches!(settings.language.as_str(), "tr" | "en") {
        return Err(HubError::Configuration("unsupported language".into()));
    }

    let autostart = app.autolaunch();
    if settings.launch_at_startup {
        autostart
            .enable()
            .map_err(|error| HubError::Operation(error.to_string()))?;
    } else {
        autostart
            .disable()
            .map_err(|error| HubError::Operation(error.to_string()))?;
    }
    state
        .paused
        .store(!settings.process_monitoring, Ordering::Relaxed);
    settings.proxy_model_map.retain(|rule| !rule.from.trim().is_empty() && !rule.to.trim().is_empty());
    settings.proxy_timeout_seconds = settings.proxy_timeout_seconds.clamp(10, 600);
    if settings.proxy_port < 1024 {
        return Err(HubError::Configuration("proxy port must be between 1024 and 65535".into()));
    }
    if !matches!(settings.antigravity_switch_strategy.as_str(), "highest_quota" | "round_robin" | "tier_priority") {
        return Err(HubError::Configuration("unsupported switch strategy".into()));
    }
    {
        let db = state
            .database
            .lock()
            .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
        // The key is only ever changed through regenerate_proxy_api_key, and the risk notice
        // only through acknowledge_proxy_risk.
        let stored = db.settings()?;
        settings.proxy_api_key = stored.proxy_api_key;
        settings.proxy_risk_acknowledged = stored.proxy_risk_acknowledged;
        db.save_settings(&settings)?;
    }
    crate::proxy::set_model_map(settings.proxy_model_map.clone());
    crate::proxy::set_proxy_options(settings.proxy_timeout_seconds, settings.proxy_background_downgrade);
    if let Err(error) = update_tray_language(&app, &settings.language) {
        tracing::warn!(%error, "could not update tray language");
    }
    Ok(settings)
}

#[tauri::command]
pub fn set_monitoring_paused(state: State<'_, HubState>, paused: bool) -> bool {
    state.paused.store(paused, Ordering::Relaxed);
    paused
}

#[tauri::command]
pub fn set_app_executable(
    state: State<'_, HubState>,
    app_id: String,
    executable_path: String,
) -> Result<()> {
    let canonical = std::fs::canonicalize(&executable_path)?;
    if !crate::platform::is_launchable(&canonical) {
        return Err(HubError::Configuration(
            if cfg!(windows) {
                "select a valid Windows .exe file"
            } else if cfg!(target_os = "macos") {
                "select an application (.app) or an executable file"
            } else {
                "select an executable file"
            }
            .into(),
        ));
    }
    let path = canonical.to_string_lossy();
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .set_executable(&app_id, &path)
}

#[tauri::command]
pub fn launch_app(state: State<'_, HubState>, app_id: String) -> Result<()> {
    launch(&state, &app_id)
}

pub fn launch(state: &HubState, app_id: &str) -> Result<()> {
    let definition = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .application(app_id)?
        .ok_or_else(|| HubError::AppNotFound(app_id.into()))?;
    let executable =
        resolve_executable(&definition).ok_or_else(|| HubError::NotLaunchable(app_id.into()))?;
    spawn_hidden(&executable, &definition.launch_arguments)
}

fn spawn_hidden(executable: &Path, arguments: &[String]) -> Result<()> {
    // An app bundle is a folder; Launch Services starts it (or brings it forward if it is open).
    if cfg!(target_os = "macos") && executable.is_dir() {
        let mut command = Command::new("/usr/bin/open");
        command.arg("-a").arg(executable);
        if !arguments.is_empty() {
            command.arg("--args").args(arguments);
        }
        command.spawn()?;
        return Ok(());
    }
    let mut command = Command::new(executable);
    command.args(arguments);
    if let Some(parent) = executable.parent() {
        command.current_dir(parent);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command.spawn()?;
    Ok(())
}

#[tauri::command(async)]
pub fn get_antigravity_accounts(state: State<'_, HubState>) -> Result<Vec<AntigravityAccount>> {
    // Local and quick: the live session sync. Google quota for the other accounts is pulled by
    // refresh_all_antigravity_accounts, so opening the page never waits on the network.
    let _ = crate::antigravity::sync_antigravity_active_session(&state.database);
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .antigravity_accounts()
}

#[tauri::command(async)]
pub fn capture_current_antigravity_account(state: State<'_, HubState>) -> Result<AntigravityAccount> {
    crate::antigravity::capture_current_account(&state.database)
}

#[tauri::command(async)]
pub fn add_antigravity_account(
    state: State<'_, HubState>,
    email: String,
    refresh_token: String,
) -> Result<AntigravityAccount> {
    let token_resp = crate::antigravity::refresh_google_oauth_token(&refresh_token, None)?;
    let now = chrono::Utc::now().timestamp();

    let (u_email, u_name) = crate::antigravity::fetch_google_account_info(&token_resp.access_token);
    let final_email = u_email.unwrap_or_else(|| email.trim().to_string());
    let (quota_pct, quota_details) = match crate::antigravity::fetch_google_quota_summary(&token_resp.access_token, None, None) {
        Some((q, d)) => (Some(q), Some(d)),
        None => (None, None),
    };

    let id = format!("antigravity:{}", final_email.to_lowercase());
    let account = AntigravityAccount {
        id: id.clone(),
        email: final_email,
        display_name: u_name,
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token.unwrap_or(refresh_token),
        id_token: token_resp.id_token,
        project_id: None,
        expiry_timestamp: now + token_resp.expires_in,
        status: if quota_pct.is_some_and(|q| q <= 0.0) { "rate_limited".into() } else { "healthy".into() },
        is_current: false,
        last_used: now,
        quota_remaining_percent: quota_pct,
        quota_details,
        proxy_url: None,
        tier: None,
    };
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    db.save_antigravity_account(&account)?;
    Ok(account)
}

#[tauri::command]
pub fn delete_antigravity_account(
    state: State<'_, HubState>,
    account_id: String,
) -> Result<bool> {
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .delete_antigravity_account(&account_id)
}

#[tauri::command(async)]
pub fn switch_antigravity_account(
    state: State<'_, HubState>,
    account_id: String,
) -> Result<AntigravityAccount> {
    crate::antigravity::switch_to_account(&state.database, &account_id)
}

#[tauri::command]
pub fn toggle_antigravity_auto_switch(
    state: State<'_, HubState>,
    enabled: bool,
) -> Result<bool> {
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    let mut settings = db.settings()?;
    settings.antigravity_auto_switch = enabled;
    db.save_settings(&settings)?;
    Ok(enabled)
}

#[tauri::command(async)]
pub fn check_for_update() -> Result<crate::updater::UpdateInfo> {
    crate::updater::check()
}

/// Starts the verified installer and exits so it can replace the running files.
#[tauri::command(async)]
pub fn install_update(app: tauri::AppHandle) -> Result<()> {
    if crate::updater::install()? {
        app.exit(0);
    }
    Ok(())
}

#[tauri::command]
pub fn get_antigravity_oauth_url() -> Result<String> {
    Ok(crate::antigravity::get_google_oauth_auth_url())
}

#[tauri::command(async)]
pub fn start_antigravity_oauth_login(
    state: State<'_, HubState>,
    open_browser: Option<bool>,
) -> Result<AntigravityAccount> {
    let auth_url = crate::antigravity::get_google_oauth_auth_url();

    // `false` = the user copied the link to open it in a browser/profile of their choice.
    if open_browser.unwrap_or(true) {
        if let Err(error) = crate::platform::open_url(&auth_url) {
            tracing::warn!(%error, "could not open the browser for Google sign-in");
        }
    }

    let code = crate::antigravity::listen_for_oauth_callback(120)?;
    complete_oauth_code_internal(&state, &code)
}

#[tauri::command(async)]
pub fn complete_antigravity_oauth_code(
    state: State<'_, HubState>,
    code: String,
) -> Result<AntigravityAccount> {
    complete_oauth_code_internal(&state, &code)
}

fn complete_oauth_code_internal(
    state: &State<'_, HubState>,
    code: &str,
) -> Result<AntigravityAccount> {
    let token_resp = crate::antigravity::exchange_google_oauth_code(code)?;
    let now = chrono::Utc::now().timestamp();

    let (mut email_opt, mut display_name) = token_resp
        .id_token
        .as_deref()
        .and_then(crate::antigravity::parse_jwt_claims)
        .map(|(e, n)| (Some(e), n))
        .unwrap_or((None, None));

    if email_opt.is_none() {
        let (u_email, u_name) = crate::antigravity::fetch_google_account_info(&token_resp.access_token);
        if let Some(e) = u_email {
            email_opt = Some(e);
        }
        if display_name.is_none() {
            display_name = u_name;
        }
    }

    let email = email_opt.unwrap_or_else(|| "google_account".into());
    let (quota_pct, quota_details) = match crate::antigravity::fetch_google_quota_summary(&token_resp.access_token, None, None) {
        Some((q, d)) => (Some(q), Some(d)),
        None => (None, None),
    };

    let id = format!("antigravity:{}", email.to_lowercase());
    let account = AntigravityAccount {
        id: id.clone(),
        email,
        display_name,
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token.unwrap_or_default(),
        id_token: token_resp.id_token,
        project_id: None,
        expiry_timestamp: now + token_resp.expires_in,
        status: if quota_pct.is_some_and(|q| q <= 0.0) { "rate_limited".into() } else { "healthy".into() },
        is_current: false,
        last_used: now,
        quota_remaining_percent: quota_pct,
        quota_details,
        proxy_url: None,
        tier: None,
    };

    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    db.save_antigravity_account(&account)?;
    Ok(account)
}

#[tauri::command]
pub fn export_antigravity_accounts(
    state: State<'_, HubState>,
) -> Result<String> {
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    crate::antigravity::export_antigravity_accounts_json(&db)
}

#[tauri::command(async)]
pub fn import_antigravity_accounts(
    state: State<'_, HubState>,
    json_data: String,
) -> Result<usize> {
    crate::antigravity::import_antigravity_accounts_json(&state.database, &json_data)
}

#[tauri::command(async)]
pub fn refresh_all_antigravity_accounts(
    state: State<'_, HubState>,
) -> Result<Vec<AntigravityAccount>> {
    crate::antigravity::refresh_all_accounts(&state.database)
}

#[tauri::command(async)]
pub fn scan_local_antigravity_accounts(
    state: State<'_, HubState>,
) -> Result<Vec<AntigravityAccount>> {
    crate::antigravity::scan_local_accounts(&state.database)
}

#[tauri::command(async)]
pub fn get_antigravity_live_session(state: State<'_, HubState>) -> Result<AntigravityLiveSession> {
    crate::antigravity::sync_antigravity_active_session(&state.database)
}

#[tauri::command(async)]
pub fn sync_antigravity_session(state: State<'_, HubState>) -> Result<AntigravityLiveSession> {
    crate::antigravity::sync_antigravity_active_session(&state.database)
}

#[tauri::command]
pub fn get_proxy_status(state: State<'_, HubState>) -> crate::proxy::ProxyStatus {
    crate::proxy::get_proxy_status(&state)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestPromptResult {
    pub text: String,
    pub latency_ms: u64,
}

#[tauri::command(async)]
pub fn test_proxy_prompt(
    state: State<'_, HubState>,
    prompt: String,
    model: Option<String>,
) -> Result<TestPromptResult> {
    let model_to_use = model.unwrap_or_else(|| "gemini-2.5-flash".into());
    let (text, latency_ms) = crate::proxy::test_prompt(&state, &prompt, &model_to_use)
        .map_err(HubError::Operation)?;
    Ok(TestPromptResult { text, latency_ms })
}

#[tauri::command]
pub fn regenerate_proxy_api_key(state: State<'_, HubState>) -> Result<AppSettings> {
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    let mut settings = db.settings()?;
    settings.proxy_api_key = crate::proxy::generate_api_key();
    db.save_settings(&settings)?;
    Ok(settings)
}

#[tauri::command(async)]
pub fn get_proxy_models(state: State<'_, HubState>) -> Vec<crate::proxy::ProxyModel> {
    crate::proxy::available_models(&state)
}

#[tauri::command]
pub fn clear_proxy_logs() -> Result<bool> {
    crate::proxy::clear_proxy_logs();
    Ok(true)
}

/// Sets the proxy one pool account's Google traffic goes through, or clears it with an empty
/// string. Returns the saved address.
#[tauri::command]
pub fn set_antigravity_account_proxy(
    state: State<'_, HubState>,
    account_id: String,
    proxy_url: String,
) -> Result<Option<String>> {
    let proxy_url = if proxy_url.trim().is_empty() { None } else { Some(crate::antigravity::validate_proxy_url(&proxy_url)?) };
    let found = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .set_antigravity_account_proxy(&account_id, proxy_url.as_deref())?;
    if !found {
        return Err(HubError::Operation("account is no longer in the pool".into()));
    }
    Ok(proxy_url)
}

/// Closes the Antigravity IDE; `force` ends it without letting it prompt for unsaved work.
#[tauri::command(async)]
pub fn close_antigravity(force: bool) -> Result<bool> {
    crate::antigravity::close_antigravity(force)
}

/// Records that the user read and accepted the proxy's account-risk notice; the proxy answers
/// requests only after this.
#[tauri::command]
pub fn acknowledge_proxy_risk(state: State<'_, HubState>) -> Result<AppSettings> {
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    let mut settings = db.settings()?;
    settings.proxy_risk_acknowledged = true;
    db.save_settings(&settings)?;
    Ok(settings)
}

/// Saves (or, with an empty key, removes) the API key of a key-based provider and refreshes the
/// dashboard so the card reflects it straight away.
#[tauri::command(async)]
pub fn set_provider_key(state: State<'_, HubState>, provider_id: String, key: String) -> Result<DashboardSnapshot> {
    if !crate::providers::keyed::KEYED_PROVIDERS.contains(&provider_id.as_str()) {
        return Err(HubError::Configuration(format!("{provider_id} does not use an API key")));
    }
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .set_provider_key(&provider_id, Some(&key))?;
    refresh_all(&state, true).map(|(snapshot, _)| snapshot)
}

fn proxy_endpoint(state: &HubState) -> Result<(String, String)> {
    let settings = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .settings()?;
    let port = crate::proxy::get_proxy_status(state).port;
    Ok((format!("http://127.0.0.1:{port}"), settings.proxy_api_key))
}

/// Which coding tools currently have the hub's proxy written into their own config file.
#[tauri::command]
pub fn get_cli_config_status() -> crate::cli_config::CliConfigStatus {
    crate::cli_config::status()
}

/// Writes the proxy address and key into a coding tool's config (`claude` or `opencode`), keeping
/// a one-time backup of the file as it was. Returns the new status.
#[tauri::command(async)]
pub fn apply_cli_config(state: State<'_, HubState>, tool: String) -> Result<crate::cli_config::CliConfigStatus> {
    let (base_url, api_key) = proxy_endpoint(&state)?;
    let models: Vec<String> = crate::proxy::available_models(&state).into_iter().map(|model| model.id).collect();
    crate::cli_config::apply(&tool, &base_url, &api_key, &models)?;
    Ok(crate::cli_config::status())
}

/// Removes the entries `apply_cli_config` added from a coding tool's config. Returns the new status.
#[tauri::command]
pub fn remove_cli_config(tool: String) -> Result<crate::cli_config::CliConfigStatus> {
    crate::cli_config::remove(&tool)?;
    Ok(crate::cli_config::status())
}
