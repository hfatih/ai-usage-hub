use std::{
    collections::HashMap,
    env, fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    error::{HubError, Result},
    models::{
        Confidence, ConnectionStatus, ProviderCapabilities, ProviderSummary, UsageSource,
        UsageValue,
    },
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountResponse {
    account: Option<Account>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    #[serde(rename = "type")]
    account_type: String,
    email: Option<String>,
    plan_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitsResponse {
    account_id: Option<String>,
    ordinary_usage_allowed: Option<bool>,
    rate_limits: RateLimitSnapshot,
    rate_limits_by_limit_id: Option<HashMap<String, RateLimitSnapshot>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitSnapshot {
    plan_type: Option<String>,
    primary: Option<RateLimitWindow>,
    secondary: Option<RateLimitWindow>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitWindow {
    used_percent: i32,
    window_duration_mins: Option<i64>,
    resets_at: Option<i64>,
}

pub fn summary() -> Result<ProviderSummary> {
    let (account_response, limits) = query_codex()?;
    let now = Utc::now().timestamp();
    if account_response.account.is_none() {
        return Ok(disconnected_summary(now));
    }

    let account = account_response.account;
    let limits = limits.ok_or_else(|| {
        HubError::Operation("Codex returned no usage data for the active account".into())
    })?;
    let active_limits = limits
        .rate_limits_by_limit_id
        .as_ref()
        .and_then(|items| {
            items
                .get("codex")
                .or_else(|| items.values().find(|item| item.primary.is_some()))
        })
        .unwrap_or(&limits.rate_limits);

    let mut usage = Vec::new();
    if let Some(window) = &active_limits.primary {
        usage.push(to_usage(window, "primary", now));
    }
    if let Some(window) = &active_limits.secondary {
        usage.push(to_usage(window, "secondary", now));
    }
    if usage.is_empty() {
        usage.push(UsageValue::unavailable("codex_quota", "Codex usage window"));
    }
    let local_tokens = recent_codex_tokens(now);
    if local_tokens > 0 {
        usage.push(UsageValue {
            metric: "codex_recent_tokens".into(),
            label: "Device tokens · all Codex accounts · last 24 hours".into(),
            value: Some(local_tokens as f64),
            max_value: None,
            unit: "tokens".into(),
            source: UsageSource::LocalLogs,
            confidence: Confidence::High,
            retrieved_at: now,
            reset_at: None,
        });
    }

    let account_label = account
        .as_ref()
        .and_then(|value| value.email.as_deref())
        .map(str::to_owned)
        .unwrap_or_else(|| "ChatGPT account".into());
    let account_key = limits.account_id.as_deref().map(str::to_owned).or_else(|| {
        account
            .as_ref()
            .and_then(|value| value.email.as_deref())
            .map(|email| format!("email:{}", email.to_lowercase()))
    });
    let plan = account
        .as_ref()
        .and_then(|value| value.plan_type.as_deref())
        .or(active_limits.plan_type.as_deref())
        .map(format_plan);

    let connected = account
        .as_ref()
        .is_some_and(|value| value.account_type == "chatgpt")
        || usage.iter().any(|value| value.value.is_some());

    Ok(ProviderSummary {
        id: "openai".into(),
        name: "ChatGPT / Codex".into(),
        account_key,
        account_label,
        plan,
        connection_status: if connected {
            ConnectionStatus::Connected
        } else {
            ConnectionStatus::NotConnected
        },
        capabilities: ProviderCapabilities {
            usage_percentage: true,
            reset_time: true,
            token_usage: true,
            local_session_detection: true,
            oauth: true,
            multiple_accounts: true,
            app_launch: true,
            rate_limit_status: true,
            ..Default::default()
        },
        usage,
        usage_allowed: limits.ordinary_usage_allowed,
        last_refresh: Some(now),
        error: None,
    })
}

fn disconnected_summary(now: i64) -> ProviderSummary {
    ProviderSummary {
        id: "openai".into(),
        name: "ChatGPT / Codex".into(),
        account_key: None,
        account_label: "Not connected".into(),
        plan: None,
        connection_status: ConnectionStatus::NotConnected,
        capabilities: ProviderCapabilities {
            usage_percentage: true,
            reset_time: true,
            token_usage: true,
            local_session_detection: true,
            oauth: true,
            multiple_accounts: true,
            app_launch: true,
            rate_limit_status: true,
            ..Default::default()
        },
        usage: vec![UsageValue::unavailable("codex_quota", "Codex usage window")],
        usage_allowed: None,
        last_refresh: Some(now),
        error: None,
    }
}

fn to_usage(window: &RateLimitWindow, fallback: &str, now: i64) -> UsageValue {
    let duration = window.window_duration_mins;
    let used_percent = if window.resets_at.is_some_and(|ts| now >= ts) {
        0 // Window has reset; quota fully restored!
    } else {
        window.used_percent
    };
    UsageValue {
        metric: duration
            .map(|minutes| format!("codex_{minutes}m_remaining"))
            .unwrap_or_else(|| format!("codex_{fallback}_remaining")),
        label: window_label(duration, fallback),
        value: Some(f64::from((100 - used_percent).clamp(0, 100))),
        max_value: Some(100.0),
        unit: "% remaining".into(),
        source: UsageSource::OfficialCli,
        confidence: Confidence::High,
        retrieved_at: now,
        reset_at: window.resets_at,
    }
}

fn recent_codex_tokens(now: i64) -> i64 {
    let Some(home) = crate::platform::home_dir() else {
        return 0;
    };
    let cutoff = now.saturating_sub(86_400);
    super::recent_jsonl_files(&home.join(".codex/sessions"), cutoff)
        .into_iter()
        .filter_map(|path| fs::File::open(path).ok())
        .map(BufReader::new)
        .map(|reader| tokens_from_codex_session(reader, cutoff))
        .sum()
}

fn tokens_from_codex_session(reader: impl BufRead, cutoff: i64) -> i64 {
    let mut previous = 0;
    let mut recent = 0;
    for value in reader
        .lines()
        .map_while(|line| line.ok())
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
    {
        let Some(total) = value
            .pointer("/payload/info/total_token_usage/total_tokens")
            .and_then(Value::as_i64)
        else {
            continue;
        };
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.timestamp());
        if timestamp.is_some_and(|timestamp| timestamp >= cutoff) {
            recent += (total - previous).max(0);
        }
        previous = total;
    }
    recent
}

fn window_label(duration: Option<i64>, fallback: &str) -> String {
    match duration {
        Some(300) => "Shared 5-hour window".into(),
        Some(10_080) => "Shared weekly window".into(),
        Some(minutes) if minutes % 1_440 == 0 => format!("{}-day window", minutes / 1_440),
        Some(minutes) if minutes % 60 == 0 => format!("{}-hour window", minutes / 60),
        Some(minutes) => format!("{minutes}-minute window"),
        None if fallback == "primary" => "Primary window".into(),
        None => "Secondary window".into(),
    }
}

fn format_plan(value: &str) -> String {
    value
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn query_codex() -> Result<(AccountResponse, Option<RateLimitsResponse>)> {
    let mut errors = Vec::new();
    for executable in codex_candidates() {
        match query_executable(&executable) {
            Ok(result) => return Ok(result),
            Err(error) => errors.push(error.to_string()),
        }
    }
    let detail = errors
        .last()
        .cloned()
        .unwrap_or_else(|| "Codex executable was not found".into());
    Err(HubError::Operation(format!(
        "Codex usage is unavailable: {detail}"
    )))
}

fn codex_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("AI_USAGE_HUB_CODEX_EXECUTABLE") {
        candidates.push(PathBuf::from(path));
    }

    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        let bin = PathBuf::from(local_app_data).join("OpenAI/Codex/bin");
        if let Ok(entries) = fs::read_dir(bin) {
            let mut installed: Vec<_> = entries
                .flatten()
                .map(|entry| entry.path().join("codex.exe"))
                .filter(|path| path.is_file())
                .collect();
            installed.sort_by_key(|path| {
                fs::metadata(path)
                    .and_then(|metadata| metadata.modified())
                    .ok()
            });
            installed.reverse();
            candidates.extend(installed);
        }
    }

    // The macOS Codex app ships its own CLI inside the bundle; Homebrew and npm installs live in
    // folders a Finder-launched app does not have on its PATH.
    let mut bundled = Vec::new();
    if cfg!(target_os = "macos") {
        bundled.push(PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"));
        if let Some(home) = crate::platform::home_dir() {
            bundled.push(home.join("Applications/Codex.app/Contents/Resources/codex"));
        }
    }
    bundled.extend(crate::platform::extra_bin_dirs().into_iter().map(|dir| dir.join("codex")));
    candidates.extend(bundled.into_iter().filter(|path| path.is_file()));
    candidates.push(PathBuf::from("codex"));
    candidates.dedup();
    candidates
}

fn query_executable(executable: &Path) -> Result<(AccountResponse, Option<RateLimitsResponse>)> {
    let mut command = Command::new(executable);
    command
        .args(["app-server", "--listen", "stdio://"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }

    let mut child = command.spawn()?;
    let result = exchange(&mut child);
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn exchange(child: &mut Child) -> Result<(AccountResponse, Option<RateLimitsResponse>)> {
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| HubError::Operation("Codex input stream is unavailable".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| HubError::Operation("Codex output stream is unavailable".into()))?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(|line| line.ok()) {
            if let Ok(message) = serde_json::from_str::<Value>(&line) {
                let _ = sender.send(message);
            }
        }
    });

    send(
        &mut stdin,
        json!({
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": { "name": "ai-usage-hub", "title": "AI Usage Hub", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": { "experimentalApi": true }
            }
        }),
    )?;
    response_result(receive(&receiver, 1, REQUEST_TIMEOUT)?)?;
    send(&mut stdin, json!({ "method": "initialized" }))?;
    send(
        &mut stdin,
        json!({ "id": 2, "method": "account/read", "params": { "refreshToken": false } }),
    )?;
    let account_value = response_result(receive(&receiver, 2, REQUEST_TIMEOUT)?)?;
    let account: AccountResponse = serde_json::from_value(account_value)
        .map_err(|error| HubError::Operation(format!("invalid Codex account response: {error}")))?;
    if account.account.is_none() {
        return Ok((account, None));
    }

    send(
        &mut stdin,
        json!({
            "id": 3,
            "method": "account/rateLimits/read",
            "params": { "excludeResetCreditDetails": true, "supportsLunaReserve": false }
        }),
    )?;
    let limits_value = response_result(receive(&receiver, 3, REQUEST_TIMEOUT)?)?;

    let limits = serde_json::from_value(limits_value)
        .map_err(|error| HubError::Operation(format!("invalid Codex usage response: {error}")))?;
    Ok((account, Some(limits)))
}

fn send(stdin: &mut ChildStdin, message: Value) -> Result<()> {
    serde_json::to_writer(&mut *stdin, &message)
        .map_err(|error| HubError::Operation(error.to_string()))?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    Ok(())
}

fn receive(receiver: &Receiver<Value>, id: i64, timeout: Duration) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(HubError::Operation("Codex usage request timed out".into()));
        }
        let message = receiver
            .recv_timeout(remaining)
            .map_err(|_| HubError::Operation("Codex usage request timed out".into()))?;
        if message.get("id").and_then(Value::as_i64) == Some(id) {
            return Ok(message);
        }
    }
}

fn response_result(message: Value) -> Result<Value> {
    if let Some(error) = message.get("error") {
        let detail = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Codex returned an error");
        return Err(HubError::Operation(detail.into()));
    }
    message
        .get("result")
        .cloned()
        .ok_or_else(|| HubError::Operation("Codex returned no result".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_windows_get_human_labels() {
        assert_eq!(window_label(Some(300), "primary"), "Shared 5-hour window");
        assert_eq!(
            window_label(Some(10_080), "secondary"),
            "Shared weekly window"
        );
    }

    #[test]
    fn used_percent_becomes_remaining_percent() {
        let value = to_usage(
            &RateLimitWindow {
                used_percent: 41,
                window_duration_mins: Some(300),
                resets_at: Some(1_800_000_000),
            },
            "primary",
            1_700_000_000,
        );
        assert_eq!(value.value, Some(59.0));
        assert_eq!(value.max_value, Some(100.0));
        assert_eq!(value.source, UsageSource::OfficialCli);
    }

    #[test]
    fn local_token_log_uses_latest_cumulative_value() {
        let data = br#"{"timestamp":"2026-09-17T23:00:00Z","payload":{"info":{"total_token_usage":{"total_tokens":120}}}}
{"timestamp":"2026-09-18T00:00:00Z","payload":{"info":{"total_token_usage":{"total_tokens":245}}}}
{"timestamp":"2026-09-18T00:00:01Z","payload":{"info":{"total_token_usage":{"total_tokens":245}}}}"#;
        assert_eq!(tokens_from_codex_session(&data[..], 1_789_689_600), 125);
    }

    #[test]
    #[ignore = "requires recent local Codex sessions"]
    fn live_local_token_totals_are_nonzero() {
        assert!(recent_codex_tokens(Utc::now().timestamp()) > 0);
    }

    #[test]
    fn signed_out_account_is_explicitly_disconnected() {
        let provider = disconnected_summary(1_700_000_000);
        assert!(matches!(
            provider.connection_status,
            ConnectionStatus::NotConnected
        ));
        assert!(provider.account_key.is_none());
    }

    #[test]
    #[ignore = "requires an installed, signed-in Codex app"]
    fn live_codex_usage_round_trip() -> Result<()> {
        let provider = summary()?;
        assert!(matches!(
            provider.connection_status,
            ConnectionStatus::Connected
        ));
        assert!(provider.usage.iter().any(|usage| usage.value.is_some()));
        Ok(())
    }
}
