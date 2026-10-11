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
    state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?
        .save_settings(&settings)?;
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
    if !canonical.is_file()
        || !canonical
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err(HubError::Configuration(
            "select a valid Windows .exe file".into(),
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
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    let _ = crate::antigravity::sync_antigravity_active_session(&db);
    let mut accounts = db.antigravity_accounts()?;
    let now = chrono::Utc::now().timestamp();
    for acc in &mut accounts {
        if !acc.refresh_token.is_empty() {
            let mut changed = false;
            if acc.expiry_timestamp <= now + 60 {
                if let Ok(token_resp) = crate::antigravity::refresh_google_oauth_token(&acc.refresh_token) {
                    acc.access_token = token_resp.access_token;
                    acc.expiry_timestamp = now + token_resp.expires_in;
                    if let Some(id_tok) = token_resp.id_token {
                        acc.id_token = Some(id_tok);
                    }
                    changed = true;
                }
            }
            let quota_opt = crate::antigravity::fetch_google_quota_summary(&acc.access_token, acc.project_id.as_deref());
            if let Some((q, details)) = quota_opt {
                if acc.quota_remaining_percent != Some(q) {
                    acc.quota_remaining_percent = Some(q);
                    changed = true;
                }
                if acc.quota_details.as_ref() != Some(&details) {
                    acc.quota_details = Some(details);
                    changed = true;
                }
                let new_status = if q <= 0.0 { "rate_limited" } else { "healthy" };
                if acc.status != new_status && acc.status != "expired" && acc.status != "forbidden" {
                    acc.status = new_status.into();
                    changed = true;
                }
            } else if acc.is_current {
                // If direct CloudCode wasn't reachable, check local session for active account
                if let Ok(summary) = crate::providers::google::summary() {
                    let mut d = crate::models::AntigravityQuotaDetails::default();
                    for u in &summary.usage {
                        let m = u.metric.to_ascii_lowercase();
                        if let Some(val) = u.value {
                            if m.contains("gemini-5h") || m.contains("gemini_5h") {
                                d.gemini_5h = Some(val);
                                d.gemini_reset_at = u.reset_at;
                            } else if m.contains("gemini-weekly") || m.contains("gemini_weekly") {
                                d.gemini_weekly = Some(val);
                            } else if m.contains("3p-5h") || m.contains("claude-5h") || m.contains("3p_5h") {
                                d.claude_5h = Some(val);
                                d.claude_reset_at = u.reset_at;
                            } else if m.contains("3p-weekly") || m.contains("claude-weekly") || m.contains("3p_weekly") {
                                d.claude_weekly = Some(val);
                            }
                        }
                    }
                    let rep = match (d.gemini_5h, d.claude_5h) {
                        (Some(g), Some(c)) => (g + c) / 2.0,
                        (Some(g), None) => g,
                        (None, Some(c)) => c,
                        (None, None) => 100.0,
                    };
                    if acc.quota_remaining_percent != Some(rep) {
                        acc.quota_remaining_percent = Some(rep);
                        changed = true;
                    }
                    if acc.quota_details.as_ref() != Some(&d) {
                        acc.quota_details = Some(d);
                        changed = true;
                    }
                }
            }
            if changed {
                let _ = db.save_antigravity_account(acc);
            }
        }
    }
    Ok(accounts)
}

#[tauri::command(async)]
pub fn capture_current_antigravity_account(state: State<'_, HubState>) -> Result<AntigravityAccount> {
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    crate::antigravity::capture_current_account(&db)
}

#[tauri::command(async)]
pub fn add_antigravity_account(
    state: State<'_, HubState>,
    email: String,
    refresh_token: String,
) -> Result<AntigravityAccount> {
    let token_resp = crate::antigravity::refresh_google_oauth_token(&refresh_token)?;
    let now = chrono::Utc::now().timestamp();

    let (u_email, u_name) = crate::antigravity::fetch_google_account_info(&token_resp.access_token);
    let final_email = u_email.unwrap_or_else(|| email.trim().to_string());
    let (quota_pct, quota_details) = match crate::antigravity::fetch_google_quota_summary(&token_resp.access_token, None) {
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
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    crate::antigravity::switch_to_account(&db, &account_id)
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

#[tauri::command]
pub fn get_antigravity_oauth_url() -> Result<String> {
    Ok(crate::antigravity::get_google_oauth_auth_url())
}

#[tauri::command(async)]
pub fn start_antigravity_oauth_login(
    state: State<'_, HubState>,
) -> Result<AntigravityAccount> {
    let auth_url = crate::antigravity::get_google_oauth_auth_url();

    #[cfg(windows)]
    {
        let _ = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", &auth_url])
            .spawn();
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
    let (quota_pct, quota_details) = match crate::antigravity::fetch_google_quota_summary(&token_resp.access_token, None) {
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
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    crate::antigravity::import_antigravity_accounts_json(&db, &json_data)
}

#[tauri::command(async)]
pub fn refresh_all_antigravity_accounts(
    state: State<'_, HubState>,
) -> Result<Vec<AntigravityAccount>> {
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    crate::antigravity::refresh_all_accounts(&db)
}

#[tauri::command(async)]
pub fn scan_local_antigravity_accounts(
    state: State<'_, HubState>,
) -> Result<Vec<AntigravityAccount>> {
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    crate::antigravity::scan_local_accounts(&db)
}

#[tauri::command(async)]
pub fn get_antigravity_live_session(state: State<'_, HubState>) -> Result<AntigravityLiveSession> {
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    crate::antigravity::sync_antigravity_active_session(&db)
}

#[tauri::command(async)]
pub fn sync_antigravity_session(state: State<'_, HubState>) -> Result<AntigravityLiveSession> {
    let db = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))?;
    crate::antigravity::sync_antigravity_active_session(&db)
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
pub fn clear_proxy_logs() -> Result<bool> {
    crate::proxy::clear_proxy_logs();
    Ok(true)
}


