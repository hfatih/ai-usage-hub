use std::{env, fs, path::PathBuf, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

use crate::{
    error::{HubError, Result},
    models::{
        Confidence, ConnectionStatus, ProviderCapabilities, ProviderSummary, UsageSource,
        UsageValue,
    },
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const QUOTA_METHOD: &str = "RetrieveUserQuotaSummary";
const STATUS_METHOD: &str = "GetUserStatus";
const QUOTA_UNIT: &str = "% remaining";

#[derive(Debug)]
struct LocalSession {
    port: u16,
    csrf_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserStatusEnvelope {
    user_status: UserStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserStatus {
    name: Option<String>,
    email: Option<String>,
    plan_status: Option<PlanStatus>,
    user_tier: Option<UserTier>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserTier {
    name: Option<String>,
    #[serde(default)]
    available_credits: Vec<TierCredit>,
}

// Credit numbers arrive as JSON numbers or int64 strings depending on the field, so they stay
// untyped until `number` reads them.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TierCredit {
    credit_type: String,
    credit_amount: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanStatus {
    plan_info: Option<PlanInfo>,
    available_prompt_credits: Option<Value>,
    available_flow_credits: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanInfo {
    plan_name: Option<String>,
    monthly_prompt_credits: Option<Value>,
    monthly_flow_credits: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct QuotaEnvelope {
    response: QuotaResponse,
}

#[derive(Debug, Deserialize)]
struct QuotaResponse {
    #[serde(default)]
    groups: Vec<QuotaGroup>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaGroup {
    display_name: String,
    #[serde(default)]
    buckets: Vec<QuotaBucket>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaBucket {
    bucket_id: String,
    display_name: String,
    remaining_fraction: f64,
    reset_time: Option<String>,
}

/// Account, plan and usage rows read from the running Antigravity language server.
#[derive(Debug, Clone)]
pub struct LiveSessionResult {
    pub email: Option<String>,
    pub name: Option<String>,
    pub plan: Option<String>,
    pub port: u16,
    pub usage: Vec<UsageValue>,
}

pub fn summary() -> Result<ProviderSummary> {
    let Some(live) = live_session().filter(|live| live.usage.iter().any(|u| u.unit == QUOTA_UNIT))
    else {
        return summary_from_vault();
    };
    let usage_allowed = live
        .usage
        .iter()
        .filter(|value| value.unit == QUOTA_UNIT)
        .filter_map(|value| value.value)
        .any(|value| value > 0.0);
    Ok(ProviderSummary {
        id: "google".into(),
        name: "Google / Antigravity".into(),
        account_key: live
            .email
            .as_deref()
            .map(|value| format!("email:{}", value.to_lowercase())),
        account_label: live
            .email
            .or(live.name)
            .unwrap_or_else(|| "Google account".into()),
        plan: live.plan,
        connection_status: ConnectionStatus::Connected,
        capabilities: capabilities(),
        usage: live.usage,
        usage_allowed: Some(usage_allowed),
        last_refresh: Some(Utc::now().timestamp()),
        error: None,
    })
}

/// Reads the signed-in account, its quota windows and its credit balances from the running
/// Antigravity language server. Returns `None` when Antigravity is closed or does not answer.
pub fn live_session() -> Option<LiveSessionResult> {
    let session = find_antigravity_session()?;
    let client = local_client()?;
    let status = request::<UserStatusEnvelope>(&client, &session, STATUS_METHOD, json!({}))
        .ok()?
        .user_status;
    let now = Utc::now().timestamp();
    let mut usage = request::<QuotaEnvelope>(&client, &session, QUOTA_METHOD, json!({ "forceRefresh": true }))
        .map(|quota| {
            quota
                .response
                .groups
                .iter()
                .flat_map(|group| group.buckets.iter().map(move |bucket| to_usage(group, bucket, now)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    usage.extend(credit_usage(&status, crate::antigravity::read_vscdb_ai_credits(), now));

    let plan = status
        .user_tier
        .and_then(|value| value.name)
        .or_else(|| status.plan_status.and_then(|value| value.plan_info).and_then(|value| value.plan_name))
        .filter(|value| !value.is_empty());
    Some(LiveSessionResult {
        email: status.email.filter(|value| !value.is_empty()),
        name: status.name.filter(|value| !value.is_empty()),
        plan,
        port: session.port,
        usage,
    })
}

/// Email of the account the running language server is signed in with; a single local request,
/// cheap enough to poll while waiting for an account switch to land.
pub fn live_account_email() -> Option<String> {
    let session = find_antigravity_session()?;
    request::<UserStatusEnvelope>(&local_client()?, &session, STATUS_METHOD, json!({}))
        .ok()?
        .user_status
        .email
        .filter(|value| !value.is_empty())
}

fn local_client() -> Option<Client> {
    Client::builder()
        .danger_accept_invalid_certs(true)
        .https_only(true)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .ok()
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse().ok())
}

/// Credit balances the language server reports. The Google One AI balance is often missing from
/// the response; `stored_ai_credits` (Antigravity's own saved state) fills it in, at lower
/// confidence, but only for a tier that lists that credit type.
fn credit_usage(status: &UserStatus, stored_ai_credits: Option<f64>, now: i64) -> Vec<UsageValue> {
    let row = |metric: &str, label: &str, value: f64, max_value: Option<f64>, confidence| UsageValue {
        metric: metric.into(),
        label: label.into(),
        value: Some(value),
        max_value,
        unit: "credits".into(),
        source: UsageSource::LocalApplication,
        confidence,
        retrieved_at: now,
        reset_at: None,
    };
    let mut rows = Vec::new();

    let ai_credit = status
        .user_tier
        .iter()
        .flat_map(|tier| &tier.available_credits)
        .find(|credit| credit.credit_type == "GOOGLE_ONE_AI");
    if let Some(credit) = ai_credit {
        let reported = credit.credit_amount.as_ref().and_then(number);
        if let Some((value, confidence)) = reported
            .map(|value| (value, Confidence::High))
            .or(stored_ai_credits.map(|value| (value, Confidence::Medium)))
        {
            rows.push(row("google_one_ai_credits", "Google One AI · AI credits", value, None, confidence));
        }
    }

    if let Some(plan) = &status.plan_status {
        let info = plan.plan_info.as_ref();
        for (metric, label, available, monthly) in [
            (
                "antigravity_prompt_credits",
                "Antigravity · Prompt credits",
                &plan.available_prompt_credits,
                info.and_then(|info| info.monthly_prompt_credits.as_ref()),
            ),
            (
                "antigravity_flow_credits",
                "Antigravity · Flow credits",
                &plan.available_flow_credits,
                info.and_then(|info| info.monthly_flow_credits.as_ref()),
            ),
        ] {
            if let Some(value) = available.as_ref().and_then(number) {
                let max_value = monthly.and_then(number).filter(|max| *max > 0.0 && *max >= value);
                rows.push(row(metric, label, value, max_value, Confidence::High));
            }
        }
    }
    rows
}

fn summary_from_vault() -> Result<ProviderSummary> {
    let cred = crate::antigravity::read_system_antigravity_credential()?
        .ok_or_else(|| HubError::Operation("Open Antigravity and sign in to refresh Google usage".into()))?;

    let now = Utc::now().timestamp();
    let mut details = cred.token;
    let mut access_token = details.access_token.clone();

    let expiry_ts = DateTime::parse_from_rfc3339(&details.expiry)
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    if now >= expiry_ts - 60 && !details.refresh_token.is_empty() {
        if let Ok(token_resp) = crate::antigravity::refresh_google_oauth_token(&details.refresh_token) {
            access_token = token_resp.access_token.clone();
            details.access_token = token_resp.access_token;
            if let Some(ref rt) = token_resp.refresh_token {
                details.refresh_token = rt.clone();
            }
            if let Some(ref it) = token_resp.id_token {
                details.id_token = Some(it.clone());
            }
            details.expiry = DateTime::from_timestamp(now + token_resp.expires_in, 0)
                .unwrap_or_else(Utc::now)
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
            let _ = crate::antigravity::write_system_antigravity_credential(&crate::antigravity::KeyringPayload {
                token: details.clone(),
                auth_method: cred.auth_method.clone(),
            });
        }
    }

    let mut usage = crate::antigravity::fetch_google_usage_buckets(&access_token, details.project_id.as_deref());
    if usage.is_empty() && !details.refresh_token.is_empty() {
        if let Ok(token_resp) = crate::antigravity::refresh_google_oauth_token(&details.refresh_token) {
            access_token = token_resp.access_token.clone();
            details.access_token = token_resp.access_token;
            let _ = crate::antigravity::write_system_antigravity_credential(&crate::antigravity::KeyringPayload {
                token: details.clone(),
                auth_method: cred.auth_method.clone(),
            });
            usage = crate::antigravity::fetch_google_usage_buckets(&access_token, details.project_id.as_deref());
        }
    }

    if usage.is_empty() {
        return Err(HubError::Operation("Antigravity returned no quota windows".into()));
    }

    let (email, name) = crate::antigravity::fetch_google_account_info(&access_token);
    let account_label = email.clone().or(name).unwrap_or_else(|| "Google account".into());
    let usage_allowed = usage.iter().filter_map(|u| u.value).any(|v| v > 0.0);

    Ok(ProviderSummary {
        id: "google".into(),
        name: "Google / Antigravity".into(),
        account_key: email.as_deref().map(|value| format!("email:{}", value.to_lowercase())),
        account_label,
        // The quota endpoint does not report the subscription tier.
        plan: None,
        connection_status: ConnectionStatus::Connected,
        capabilities: capabilities(),
        usage,
        usage_allowed: Some(usage_allowed),
        last_refresh: Some(now),
        error: None,
    })
}

fn request<T: DeserializeOwned>(
    client: &Client,
    session: &LocalSession,
    method: &str,
    body: serde_json::Value,
) -> Result<T> {
    let url = format!(
        "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/{method}",
        session.port
    );
    client
        .post(url)
        .header("x-codeium-csrf-token", &session.csrf_token)
        .header("Connect-Protocol-Version", "1")
        .json(&body)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(operation)?
        .json()
        .map_err(operation)
}

fn to_usage(group: &QuotaGroup, bucket: &QuotaBucket, now: i64) -> UsageValue {
    let reset_at = bucket
        .reset_time
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp());

    let fraction = if reset_at.is_some_and(|ts| now >= ts) {
        1.0 // Reset time has arrived; quota fully renewed!
    } else {
        bucket.remaining_fraction
    };

    UsageValue {
        metric: format!("antigravity_{}_remaining", bucket.bucket_id),
        label: format!("{} · {}", group.display_name, bucket.display_name),
        value: Some((fraction * 100.0).clamp(0.0, 100.0)),
        max_value: Some(100.0),
        unit: QUOTA_UNIT.into(),
        source: UsageSource::LocalApplication,
        confidence: Confidence::High,
        retrieved_at: now,
        reset_at,
    }
}

fn find_antigravity_session() -> Option<LocalSession> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_exe(sysinfo::UpdateKind::OnlyIfNotSet)
            .with_cmd(sysinfo::UpdateKind::Always),
    );

    let csrf_token = system
        .processes()
        .values()
        .find_map(|process| {
            let name_lower = process.name().to_string_lossy().to_ascii_lowercase();
            let executable = process
                .exe()
                .map(|path| path.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let is_ls = name_lower.starts_with("language_server");
            let is_antigravity = executable.contains("antigravity")
                || process.cmd().iter().any(|c| c.to_string_lossy().to_ascii_lowercase().contains("antigravity"));
            if !is_ls || !is_antigravity {
                return None;
            }
            command_value(process.cmd(), "--csrf_token")
        })
        .filter(|value| valid_csrf_token(value))?;
    let port = antigravity_log_candidates()
        .into_iter()
        .find_map(|path| https_port_from_log(&path))?;
    Some(LocalSession { port, csrf_token })
}

fn command_value(parts: &[std::ffi::OsString], flag: &str) -> Option<String> {
    parts.iter().enumerate().find_map(|(index, part)| {
        let value = part.to_string_lossy();
        if value == flag {
            parts
                .get(index + 1)
                .map(|next| next.to_string_lossy().into_owned())
        } else {
            value.strip_prefix(&format!("{flag}=")).map(str::to_owned)
        }
    })
}

fn valid_csrf_token(value: &str) -> bool {
    (16..=128).contains(&value.len())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

/// Language server logs of every Antigravity flavour, most recently written first: the running
/// server is the one still appending, and a leftover log from the other flavour holds a dead port.
fn antigravity_log_candidates() -> Vec<PathBuf> {
    let Some(app_data) = env::var_os("APPDATA") else {
        return Vec::new();
    };
    let root = PathBuf::from(app_data);
    let mut logs = vec![
        root.join("Antigravity/logs/language_server.log"),
        root.join("Antigravity IDE/logs/language_server.log"),
    ];
    logs.sort_by_key(|path| std::cmp::Reverse(fs::metadata(path).and_then(|meta| meta.modified()).ok()));
    logs
}

fn https_port_from_log(path: &PathBuf) -> Option<u16> {
    let content = fs::read_to_string(path).ok()?;
    content.lines().rev().find_map(|line| {
        if !line.contains("for HTTPS") {
            return None;
        }
        line.split_once("random port at ")?
            .1
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

fn capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        usage_percentage: true,
        reset_time: true,
        local_session_detection: true,
        oauth: true,
        multiple_accounts: true,
        app_launch: true,
        rate_limit_status: true,
        ..Default::default()
    }
}

fn operation(error: impl std::fmt::Display) -> HubError {
    HubError::Operation(format!("Antigravity usage is unavailable: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_fraction_becomes_remaining_percent() {
        let value = to_usage(
            &QuotaGroup {
                display_name: "Gemini Models".into(),
                buckets: Vec::new(),
            },
            &QuotaBucket {
                bucket_id: "gemini-5h".into(),
                display_name: "Five Hour Limit Remaining".into(),
                remaining_fraction: 0.938,
                reset_time: Some("2026-09-18T03:17:36Z".into()),
            },
            1_700_000_000,
        );
        assert_eq!(value.value, Some(93.8));
        assert_eq!(value.reset_at, Some(1_789_701_456));
        assert_eq!(value.source, UsageSource::LocalApplication);
    }

    // Shape of a real GetUserStatus response: the tier lists the credit type with only its
    // usage threshold, never a balance.
    fn status(tier_credit: Value) -> UserStatus {
        serde_json::from_value::<UserStatusEnvelope>(json!({ "userStatus": {
            "email": "user@example.com",
            "planStatus": {
                "planInfo": { "planName": "Pro", "monthlyPromptCredits": 50000, "monthlyFlowCredits": 150000 },
                "availablePromptCredits": 500,
                "availableFlowCredits": 100
            },
            "userTier": { "name": "Google AI Pro", "availableCredits": [tier_credit] }
        }}))
        .unwrap()
        .user_status
    }

    #[test]
    fn credits_never_show_the_usage_threshold_as_a_balance() {
        let threshold_only = json!({ "creditType": "GOOGLE_ONE_AI", "minimumCreditAmountForUsage": "50" });
        let value = |rows: &[UsageValue], metric: &str| rows.iter().find(|row| row.metric == metric).and_then(|row| row.value);

        let rows = credit_usage(&status(threshold_only.clone()), None, 0);
        assert_eq!(value(&rows, "google_one_ai_credits"), None);
        assert_eq!(value(&rows, "antigravity_prompt_credits"), Some(500.0));
        assert_eq!(value(&rows, "antigravity_flow_credits"), Some(100.0));
        assert!(rows.iter().all(|row| row.validate().is_ok()));

        let rows = credit_usage(&status(threshold_only), Some(1000.0), 0);
        assert_eq!(value(&rows, "google_one_ai_credits"), Some(1000.0));

        let reported = json!({ "creditType": "GOOGLE_ONE_AI", "creditAmount": "750", "minimumCreditAmountForUsage": "50" });
        let rows = credit_usage(&status(reported), Some(1000.0), 0);
        assert_eq!(value(&rows, "google_one_ai_credits"), Some(750.0));
    }

    #[test]
    fn command_values_support_split_and_equals_forms() {
        let split = ["server.exe", "--csrf_token", "abc-def-0123456789"]
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        let equals = ["server.exe", "--csrf_token=abc-def-0123456789"]
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        assert_eq!(
            command_value(&split, "--csrf_token").as_deref(),
            Some("abc-def-0123456789")
        );
        assert_eq!(
            command_value(&equals, "--csrf_token").as_deref(),
            Some("abc-def-0123456789")
        );
    }

    #[test]
    #[ignore = "requires running Antigravity app"]
    fn test_find_session() {
        let sess = find_antigravity_session();
        println!("FOUND SESSION: {:?}", sess.map(|session| session.port));
        let s = summary().expect("summary should succeed");
        println!("SUMMARY: plan={:?}, usage count={}", s.plan, s.usage.len());
        for u in &s.usage {
            println!("  -> metric={} label={} value={:?} reset_at={:?}", u.metric, u.label, u.value, u.reset_at);
        }
    }
}
