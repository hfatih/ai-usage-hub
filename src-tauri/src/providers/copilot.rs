//! GitHub Copilot usage. Uses the sign-in the Copilot editor plugins keep in
//! `github-copilot` folder (see `copilot_config_dir`) and the account endpoint those plugins call for their own
//! quota display. The endpoint is not publicly documented, so rows carry medium confidence.

use std::{path::PathBuf, time::Duration};

use chrono::{DateTime, NaiveDate, Utc};
use reqwest::blocking::Client;
use serde_json::Value;

use crate::{
    error::{HubError, Result},
    models::{Confidence, ConnectionStatus, ProviderCapabilities, ProviderSummary, UsageSource, UsageValue},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const SIGN_IN_HINT: &str = "Sign in to GitHub Copilot in VS Code or a JetBrains IDE to read Copilot quota";

/// Live Copilot summary: GitHub login, plan and the remaining share of each metered quota
/// (chat, completions, premium requests). Unlimited quotas are left out rather than shown as
/// 100%. Fails with a sign-in hint when no editor plugin has signed in on this PC.
pub fn summary() -> Result<ProviderSummary> {
    let (user, token) = stored_sign_in().ok_or_else(|| operation(SIGN_IN_HINT))?;
    let client = Client::builder().timeout(REQUEST_TIMEOUT).build().map_err(operation)?;
    let response = client
        .get("https://api.github.com/copilot_internal/user")
        .header(reqwest::header::AUTHORIZATION, format!("token {token}"))
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "GitHubCopilotChat/0.26.7")
        .header("Editor-Version", "vscode/1.99.0")
        .header("Editor-Plugin-Version", "copilot-chat/0.26.7")
        .send()
        .map_err(operation)?;
    if response.status().as_u16() == 401 {
        return Err(operation("GitHub Copilot sign-in has expired; sign in again in your editor"));
    }
    if !response.status().is_success() {
        return Err(operation(format!("GitHub rejected the Copilot quota request ({})", response.status().as_u16())));
    }
    let body: Value = response.json().map_err(operation)?;
    let now = Utc::now().timestamp();
    let usage = usage_from_account(&body, now);
    let login = body.get("login").and_then(Value::as_str).map(str::to_owned).or(user).unwrap_or_else(|| "GitHub account".into());
    Ok(ProviderSummary {
        id: "github".into(),
        name: "GitHub Copilot".into(),
        account_key: Some(format!("login:{}", login.to_lowercase())),
        account_label: login,
        plan: body.get("copilot_plan").and_then(Value::as_str).map(str::to_owned),
        connection_status: ConnectionStatus::Connected,
        capabilities: capabilities(),
        usage_allowed: usage.iter().filter_map(|row| row.value).reduce(f64::min).map(|lowest| lowest > 0.0),
        usage,
        last_refresh: Some(now),
        error: None,
    })
}

/// What the Copilot card can show once a sign-in is found.
pub fn capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        usage_percentage: true,
        reset_time: true,
        request_count: true,
        local_session_detection: true,
        oauth: true,
        app_launch: true,
        ..Default::default()
    }
}

/// The plugins' shared sign-in folder: `%LOCALAPPDATA%\github-copilot` on Windows,
/// `$XDG_CONFIG_HOME/github-copilot` (or `~/.config/github-copilot`) on macOS and Linux.
fn copilot_config_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| crate::platform::home_dir().map(|home| home.join(".config")))?
    };
    Some(base.join("github-copilot"))
}

/// `(GitHub login, OAuth token)` from the plugins' `apps.json` or the older `hosts.json`.
fn stored_sign_in() -> Option<(Option<String>, String)> {
    let root = copilot_config_dir()?;
    ["apps.json", "hosts.json"].iter().find_map(|file| {
        let entries: Value = serde_json::from_str(&std::fs::read_to_string(root.join(file)).ok()?).ok()?;
        sign_in_from_entries(&entries)
    })
}

fn sign_in_from_entries(entries: &Value) -> Option<(Option<String>, String)> {
    entries.as_object()?.iter().filter(|(host, _)| host.starts_with("github.com")).find_map(|(_, entry)| {
        let token = entry.get("oauth_token")?.as_str().filter(|token| !token.is_empty())?;
        Some((entry.get("user").and_then(Value::as_str).map(str::to_owned), token.to_owned()))
    })
}

fn reset_timestamp(body: &Value) -> Option<i64> {
    ["quota_reset_date_utc", "quota_reset_date", "limited_user_reset_date"].iter().find_map(|key| {
        let text = body.get(*key)?.as_str()?;
        DateTime::parse_from_rfc3339(text)
            .map(|time| time.timestamp())
            .ok()
            .or_else(|| Some(NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?.and_hms_opt(0, 0, 0)?.and_utc().timestamp()))
    })
}

fn quota_label(key: &str) -> &str {
    match key {
        "chat" => "Copilot chat",
        "completions" => "Copilot code completions",
        "premium_interactions" => "Copilot premium requests",
        other => other,
    }
}

/// Rows from the account answer. Paid plans report `quota_snapshots` with a remaining
/// percentage; the free plan reports remaining and monthly request counts instead.
fn usage_from_account(body: &Value, now: i64) -> Vec<UsageValue> {
    let reset_at = reset_timestamp(body);
    let row = |key: &str, value: f64, max_value: f64, unit: &str| UsageValue {
        metric: format!("copilot_{key}"),
        label: quota_label(key).to_owned(),
        value: Some(value.clamp(0.0, max_value)),
        max_value: Some(max_value),
        unit: unit.into(),
        source: UsageSource::OfficialApi,
        confidence: Confidence::Medium,
        retrieved_at: now,
        reset_at,
    };
    if let Some(snapshots) = body.get("quota_snapshots").and_then(Value::as_object) {
        return snapshots
            .iter()
            .filter(|(_, snapshot)| snapshot.get("unlimited").and_then(Value::as_bool) != Some(true))
            .filter_map(|(key, snapshot)| Some(row(key, snapshot.get("percent_remaining")?.as_f64()?, 100.0, "% remaining")))
            .collect();
    }
    let (Some(remaining), Some(monthly)) = (
        body.get("limited_user_quotas").and_then(Value::as_object),
        body.get("monthly_quotas").and_then(Value::as_object),
    ) else {
        return Vec::new();
    };
    remaining
        .iter()
        .filter_map(|(key, left)| {
            let total = monthly.get(key)?.as_f64().filter(|total| *total > 0.0)?;
            Some(row(key, left.as_f64()?, total, "requests left"))
        })
        .collect()
}

fn operation(error: impl std::fmt::Display) -> HubError {
    HubError::Operation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sign_in_is_read_from_plugin_files() {
        let apps = json!({ "github.com:Iv1.abc": { "user": "octocat", "oauth_token": "tok" }, "other.host": { "oauth_token": "x" } });
        assert_eq!(sign_in_from_entries(&apps), Some((Some("octocat".into()), "tok".into())));
        assert_eq!(sign_in_from_entries(&json!({ "github.com": { "oauth_token": "" } })), None);
    }

    #[test]
    fn paid_and_free_plans_map_to_rows() {
        let paid = json!({ "quota_reset_date": "2030-01-01", "quota_snapshots": {
            "chat": { "unlimited": true, "percent_remaining": 100.0 },
            "premium_interactions": { "unlimited": false, "percent_remaining": 42.5 }
        }});
        let rows = usage_from_account(&paid, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].value, rows[0].unit.as_str(), rows[0].reset_at), (Some(42.5), "% remaining", Some(1_893_456_000)));

        let free = json!({ "limited_user_quotas": { "chat": 30, "completions": 1500 }, "monthly_quotas": { "chat": 50, "completions": 2000 } });
        let rows = usage_from_account(&free, 0);
        let chat = rows.iter().find(|row| row.metric == "copilot_chat").unwrap();
        assert_eq!((chat.value, chat.max_value), (Some(30.0), Some(50.0)));
        assert!(rows.iter().all(|row| row.validate().is_ok()));
        assert!(usage_from_account(&json!({}), 0).is_empty());
    }

    /// Run with `cargo test live_copilot -- --ignored --nocapture` on a PC that is signed in.
    #[test]
    #[ignore]
    fn live_copilot_summary_round_trip() {
        match summary() {
            Ok(summary) => {
                println!("plan {:?}, {} rows", summary.plan, summary.usage.len());
                for row in &summary.usage {
                    println!("  {} = {:?} {} (reset {:?})", row.label, row.value, row.unit, row.reset_at);
                    assert!(row.validate().is_ok());
                }
            }
            Err(error) => println!("not available: {error}"),
        }
    }
}
