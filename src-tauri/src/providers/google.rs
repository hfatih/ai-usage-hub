use std::{env, fs, path::PathBuf, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TierCredit {
    credit_type: String,
    minimum_credit_amount_for_usage: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanStatus {
    plan_info: Option<PlanInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanInfo {
    plan_name: Option<String>,
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

pub fn summary() -> Result<ProviderSummary> {
    if let Ok(Some(session)) = find_antigravity_session() {
        if let Ok(client) = Client::builder()
            .danger_accept_invalid_certs(true)
            .https_only(true)
            .timeout(REQUEST_TIMEOUT)
            .build()
        {
            if let Ok(status) = request::<UserStatusEnvelope>(&client, &session, STATUS_METHOD, json!({})) {
                if let Ok(quota) = request::<QuotaEnvelope>(&client, &session, QUOTA_METHOD, json!({ "forceRefresh": true })) {
                    let now = Utc::now().timestamp();
                    let mut usage = quota
                        .response
                        .groups
                        .iter()
                        .flat_map(|group| {
                            group
                                .buckets
                                .iter()
                                .map(move |bucket| to_usage(group, bucket, now))
                        })
                        .collect::<Vec<_>>();

                    // Imagen 3 & Multimodal image generation metric
                    let gemini_image_remaining = usage
                        .iter()
                        .find(|u| u.metric.contains("gemini-5h") || u.metric.contains("gemini_5h"))
                        .or_else(|| usage.iter().find(|u| u.metric.contains("gemini")))
                        .and_then(|u| u.value)
                        .unwrap_or(100.0);

                    usage.push(UsageValue {
                        metric: "antigravity_image_generation".into(),
                        label: "Imagen 3 & Multimodal · Image Generation (1:1)".into(),
                        value: Some(gemini_image_remaining),
                        max_value: Some(100.0),
                        unit: "% remaining".into(),
                        source: UsageSource::LocalApplication,
                        confidence: Confidence::High,
                        retrieved_at: now,
                        reset_at: None,
                    });

                    // Flow Credits (availableCredits from Google One AI)
                    if let Some(ref tier) = status.user_status.user_tier {
                        for credit in &tier.available_credits {
                            let amount = credit
                                .minimum_credit_amount_for_usage
                                .as_deref()
                                .and_then(|s| s.parse::<f64>().ok())
                                .unwrap_or(50.0);
                            usage.push(UsageValue {
                                metric: format!("google_credit_{}", credit.credit_type.to_lowercase()),
                                label: format!("Google One AI · {} Credits (Flow)", credit.credit_type.replace('_', " ")),
                                value: Some(amount),
                                max_value: None,
                                unit: "credits".into(),
                                source: UsageSource::LocalApplication,
                                confidence: Confidence::High,
                                retrieved_at: now,
                                reset_at: None,
                            });
                        }
                    }

                    if !usage.is_empty() {
                        let email = status.user_status.email.filter(|value| !value.is_empty());
                        let account_label = email
                            .clone()
                            .or(status.user_status.name)
                            .unwrap_or_else(|| "Google account".into());
                        let plan = status
                            .user_status
                            .user_tier
                            .and_then(|value| value.name)
                            .or_else(|| {
                                status
                                    .user_status
                                    .plan_status
                                    .and_then(|value| value.plan_info)
                                    .and_then(|value| value.plan_name)
                            })
                            .filter(|value| !value.is_empty());
                        let usage_allowed = usage
                            .iter()
                            .filter_map(|value| value.value)
                            .any(|value| value > 0.0);

                        return Ok(ProviderSummary {
                            id: "google".into(),
                            name: "Google / Antigravity".into(),
                            account_key: email
                                .as_deref()
                                .map(|value| format!("email:{}", value.to_lowercase())),
                            account_label,
                            plan,
                            connection_status: ConnectionStatus::Connected,
                            capabilities: capabilities(),
                            usage,
                            usage_allowed: Some(usage_allowed),
                            last_refresh: Some(now),
                            error: None,
                        });
                    }
                }
            }
        }
    }

    summary_from_vault()
}

#[derive(Debug, Clone)]
pub struct LiveSessionResult {
    pub email: Option<String>,
    pub name: Option<String>,
    pub plan: Option<String>,
    pub port: u16,
    pub usage: Vec<UsageValue>,
}

pub fn probe_antigravity_live_session() -> Option<LiveSessionResult> {
    let session = find_antigravity_session().ok()??;
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .https_only(true)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .ok()?;
    let status = request::<UserStatusEnvelope>(&client, &session, STATUS_METHOD, json!({})).ok()?;
    let email = status.user_status.email.filter(|value| !value.is_empty());
    let name = status.user_status.name.filter(|value| !value.is_empty());
    let plan = status
        .user_status
        .user_tier
        .and_then(|value| value.name)
        .or_else(|| {
            status
                .user_status
                .plan_status
                .and_then(|value| value.plan_info)
                .and_then(|value| value.plan_name)
        })
        .filter(|value| !value.is_empty());

    let mut usage = Vec::new();
    if let Ok(quota) = request::<QuotaEnvelope>(&client, &session, QUOTA_METHOD, json!({ "forceRefresh": true })) {
        let now = Utc::now().timestamp();
        usage = quota
            .response
            .groups
            .iter()
            .flat_map(|group| {
                group
                    .buckets
                    .iter()
                    .map(move |bucket| to_usage(group, bucket, now))
            })
            .collect();
    }

    Some(LiveSessionResult {
        email,
        name,
        plan,
        port: session.port,
        usage,
    })
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
        plan: Some("Antigravity Pro".into()),
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
        unit: "% remaining".into(),
        source: UsageSource::LocalApplication,
        confidence: Confidence::High,
        retrieved_at: now,
        reset_at,
    }
}

fn find_antigravity_session() -> Result<Option<LocalSession>> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_exe(sysinfo::UpdateKind::OnlyIfNotSet)
            .with_cmd(sysinfo::UpdateKind::Always),
    );

    let token = system.processes().values().find_map(|process| {
        let name_lower = process.name().to_string_lossy().to_ascii_lowercase();
        let executable = process
            .exe()
            .map(|path| path.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let is_ls = name_lower.starts_with("language_server") || name_lower == "language_server.exe";
        let is_antigravity = executable.contains("antigravity")
            || process.cmd().iter().any(|c| c.to_string_lossy().to_ascii_lowercase().contains("antigravity"));
        if !is_ls || !is_antigravity {
            return None;
        }
        command_value(process.cmd(), "--csrf_token")
    });
    let Some(csrf_token) = token.filter(|value| valid_csrf_token(value)) else {
        return Ok(None);
    };
    let Some(port) = antigravity_log_candidates()
        .into_iter()
        .find_map(|path| https_port_from_log(&path))
    else {
        return Ok(None);
    };
    Ok(Some(LocalSession { port, csrf_token }))
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

fn antigravity_log_candidates() -> Vec<PathBuf> {
    let Some(app_data) = env::var_os("APPDATA") else {
        return Vec::new();
    };
    let root = PathBuf::from(app_data);
    vec![
        root.join("Antigravity/logs/language_server.log"),
        root.join("Antigravity IDE/logs/language_server.log"),
    ]
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
        let sess = find_antigravity_session().unwrap();
        println!("FOUND SESSION: {:?}", sess);
        let s = summary().expect("summary should succeed");
        println!("SUMMARY: account_label={}, plan={:?}, usage count={}", s.account_label, s.plan, s.usage.len());
        for u in &s.usage {
            println!("  -> metric={} label={} value={:?} reset_at={:?}", u.metric, u.label, u.value, u.reset_at);
        }
    }
}
