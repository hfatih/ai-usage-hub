use std::{
    collections::HashMap,
    env, fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    error::{HubError, Result},
    models::{
        Confidence, ConnectionStatus, ProviderCapabilities, ProviderSummary, UsageSource,
        UsageValue,
    },
};

pub fn summary() -> Result<ProviderSummary> {
    let now = Utc::now().timestamp();
    let status = query_auth_status().unwrap_or(Value::Null);
    let cached = cached_account();
    let cached_organization = cached
        .as_ref()
        .and_then(|account| account.organization_id.as_deref());
    let desktop_usage = desktop_usage(cached_organization);
    let logged_in = status
        .get("loggedIn")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !logged_in && cached.is_none() && desktop_usage.is_none() {
        return Ok(ProviderSummary {
            id: "anthropic".into(),
            name: "Anthropic / Claude".into(),
            account_key: None,
            account_label: "Claude not connected".into(),
            plan: None,
            connection_status: ConnectionStatus::NotConnected,
            capabilities: capabilities(),
            usage: vec![UsageValue::unavailable(
                "claude_quota",
                "Shared Claude quota",
            )],
            usage_allowed: None,
            last_refresh: Some(now),
            error: None,
        });
    }

    let email = string_field(&status, &["email", "accountEmail", "userEmail"])
        .or_else(|| cached.as_ref().and_then(|value| value.email.clone()));
    let organization = string_field(&status, &["organizationId", "orgId", "organization"])
        .or_else(|| {
            cached
                .as_ref()
                .and_then(|value| value.organization_id.clone())
        })
        .or_else(|| {
            desktop_usage
                .as_ref()
                .map(|value| value.organization_id.clone())
        });
    let account_label = email
        .clone()
        .or_else(|| string_field(&status, &["displayName", "accountName"]))
        .or_else(|| cached.as_ref().and_then(|value| value.display_name.clone()))
        .unwrap_or_else(|| "Saved Claude account".into());
    let account_key = email
        .as_ref()
        .map(|value| format!("email:{}", value.to_lowercase()))
        .or_else(|| cached.as_ref().and_then(|value| value.account_id.clone()))
        .or(organization)
        .or_else(|| Some(format!("claude-code:{account_label}")));
    let plan = string_field(
        &status,
        &["subscriptionType", "planType", "plan", "accountType"],
    )
    .or_else(|| cached.as_ref().and_then(|value| value.subscription.clone()))
    .map(|value| format_plan(&value));
    let plan = plan.map(|value| format!("{value} · Shared Web / Desktop / Code"));
    let mut usage = desktop_usage
        .as_ref()
        .map(CachedClaudeUsage::usage_values)
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| {
            vec![UsageValue::unavailable(
                "claude_quota",
                if logged_in {
                    "Shared Claude quota · open Claude Desktop to refresh"
                } else {
                    "Shared Claude quota · Claude sign-in required"
                },
            )]
        });
    let has_recent_desktop_usage = desktop_usage
        .as_ref()
        .is_some_and(|value| now.saturating_sub(value.retrieved_at) <= 86_400);
    let usage_allowed = usage
        .iter()
        .filter(|value| value.unit == "% remaining")
        .filter_map(|value| value.value)
        .min_by(f64::total_cmp)
        .map(|remaining| remaining > 0.0);
    let local_tokens = recent_claude_tokens(now);
    if local_tokens > 0 {
        usage.push(UsageValue {
            metric: "claude_recent_tokens".into(),
            label: "Device tokens · all Claude accounts · last 24 hours".into(),
            value: Some(local_tokens as f64),
            max_value: None,
            unit: "tokens".into(),
            source: UsageSource::LocalLogs,
            confidence: Confidence::High,
            retrieved_at: now,
            reset_at: None,
        });
    }

    Ok(ProviderSummary {
        id: "anthropic".into(),
        name: "Anthropic / Claude".into(),
        account_key,
        account_label,
        plan,
        connection_status: if logged_in || has_recent_desktop_usage {
            ConnectionStatus::Connected
        } else {
            ConnectionStatus::NotConnected
        },
        capabilities: capabilities(),
        usage,
        usage_allowed,
        last_refresh: desktop_usage
            .as_ref()
            .map(|value| value.retrieved_at)
            .or(Some(now)),
        error: None,
    })
}

#[derive(Debug, Deserialize)]
struct DesktopUsageHistory {
    #[serde(default)]
    samples: Vec<DesktopUsageSample>,
}

#[derive(Debug, Deserialize)]
struct DesktopUsageSample {
    t: i64,
    org: String,
    u: DesktopUsagePercent,
}

#[derive(Debug, Deserialize)]
struct DesktopUsagePercent {
    fh: Option<f64>,
    sd: Option<f64>,
}

#[derive(Debug)]
struct CachedClaudeUsage {
    organization_id: String,
    retrieved_at: i64,
    five_hour_used: Option<f64>,
    seven_day_used: Option<f64>,
    five_hour_reset: Option<i64>,
    seven_day_reset: Option<i64>,
}

impl CachedClaudeUsage {
    fn usage_values(&self) -> Vec<UsageValue> {
        let now = Utc::now().timestamp();
        [
            (
                "claude_300m_remaining",
                "Shared 5-hour window",
                if self.five_hour_reset.is_some_and(|ts| now >= ts) {
                    Some(0.0)
                } else {
                    self.five_hour_used
                },
                self.five_hour_reset,
            ),
            (
                "claude_10080m_remaining",
                "Shared weekly window",
                if self.seven_day_reset.is_some_and(|ts| now >= ts) {
                    Some(0.0)
                } else {
                    self.seven_day_used
                },
                self.seven_day_reset,
            ),
        ]
        .into_iter()
        .filter_map(|(metric, label, used, reset_at)| {
            let used = used.filter(|value| (0.0..=100.0).contains(value))?;
            Some(UsageValue {
                metric: metric.into(),
                label: label.into(),
                value: Some(100.0 - used),
                max_value: Some(100.0),
                unit: "% remaining".into(),
                source: if reset_at.is_some() {
                    UsageSource::LocallyCalculated
                } else {
                    UsageSource::LocalApplication
                },
                confidence: if reset_at.is_some() {
                    Confidence::Medium
                } else {
                    Confidence::High
                },
                retrieved_at: self.retrieved_at,
                reset_at,
            })
        })
        .collect()
    }
}

fn desktop_usage(expected_organization: Option<&str>) -> Option<CachedClaudeUsage> {
    let app_data = env::var_os("APPDATA")?;
    let bytes = fs::read(PathBuf::from(app_data).join("Claude/plan-usage-history.json")).ok()?;
    desktop_usage_from_bytes(&bytes, expected_organization)
}

fn desktop_usage_from_bytes(
    bytes: &[u8],
    expected_organization: Option<&str>,
) -> Option<CachedClaudeUsage> {
    let history: DesktopUsageHistory = serde_json::from_slice(bytes).ok()?;
    let mut samples = history
        .samples
        .into_iter()
        .filter(|sample| {
            expected_organization.is_none_or(|organization| sample.org == organization)
        })
        .collect::<Vec<_>>();
    samples.sort_by_key(|sample| sample.t);
    let sample = samples.last()?;
    let (five_hour_reset, seven_day_reset) =
        inferred_resets(&samples, &sample.org, Utc::now().timestamp());
    Some(CachedClaudeUsage {
        organization_id: sample.org.clone(),
        retrieved_at: sample_time(sample),
        five_hour_used: sample.u.fh,
        seven_day_used: sample.u.sd,
        five_hour_reset,
        seven_day_reset,
    })
}

fn sample_time(sample: &DesktopUsageSample) -> i64 {
    if sample.t > 10_000_000_000 {
        sample.t / 1_000
    } else {
        sample.t
    }
}

fn inferred_resets(
    samples: &[DesktopUsageSample],
    org: &str,
    now: i64,
) -> (Option<i64>, Option<i64>) {
    let samples = samples
        .iter()
        .filter(|sample| sample.org == org)
        .collect::<Vec<_>>();
    if samples
        .last()
        .is_none_or(|sample| now - sample_time(sample) > 86_400)
    {
        return (None, None);
    }
    let five_hour = samples.windows(2).rev().find_map(|pair| {
        let (before, after) = (pair[0].u.fh?, pair[1].u.fh?);
        let reset = sample_time(pair[1]) + 5 * 3_600;
        (before >= 50.0 && after <= 20.0 && reset > now && reset <= now + 5 * 3_600)
            .then_some(reset)
    });
    let weekly_drops = samples
        .windows(2)
        .filter_map(|pair| {
            let (before, after) = (pair[0].u.sd?, pair[1].u.sd?);
            (before >= 80.0 && after <= 20.0).then(|| sample_time(pair[1]))
        })
        .collect::<Vec<_>>();
    let weekly = weekly_drops.windows(2).rev().find_map(|pair| {
        let gap = pair[1] - pair[0];
        let reset = pair[1] + 7 * 86_400;
        ((6 * 86_400..=8 * 86_400).contains(&gap) && reset > now && reset <= now + 7 * 86_400)
            .then_some(reset)
    });
    (five_hour, weekly)
}

fn recent_claude_tokens(now: i64) -> i64 {
    let Some(home) = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")) else {
        return 0;
    };
    let cutoff = now.saturating_sub(86_400);
    let mut messages = HashMap::new();
    super::recent_jsonl_files(&PathBuf::from(home).join(".claude/projects"), cutoff)
        .into_iter()
        .filter_map(|path| fs::File::open(path).ok())
        .map(BufReader::new)
        .for_each(|reader| collect_claude_tokens(reader, cutoff, &mut messages));
    messages.values().map(|(_, tokens)| tokens).sum()
}

fn collect_claude_tokens(
    reader: impl BufRead,
    cutoff: i64,
    messages: &mut HashMap<String, (i64, i64)>,
) {
    for value in reader
        .lines()
        .map_while(|line| line.ok())
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
    {
        let Some(timestamp) = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.timestamp())
            .filter(|timestamp| *timestamp >= cutoff)
        else {
            continue;
        };
        let Some(message_id) = value.pointer("/message/id").and_then(Value::as_str) else {
            continue;
        };
        let Some(usage) = value.pointer("/message/usage") else {
            continue;
        };
        let tokens = [
            "input_tokens",
            "cache_creation_input_tokens",
            "cache_read_input_tokens",
            "output_tokens",
        ]
        .into_iter()
        .filter_map(|field| usage.get(field).and_then(Value::as_i64))
        .sum::<i64>();
        if messages
            .get(message_id)
            .is_none_or(|(recorded_at, _)| timestamp >= *recorded_at)
        {
            messages.insert(message_id.into(), (timestamp, tokens));
        }
    }
}

#[derive(Debug)]
struct CachedClaudeAccount {
    account_id: Option<String>,
    email: Option<String>,
    display_name: Option<String>,
    organization_id: Option<String>,
    subscription: Option<String>,
}

fn cached_account() -> Option<CachedClaudeAccount> {
    let home = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME"))?;
    let home = PathBuf::from(home);
    let profile = read_json(&home.join(".claude.json"));
    cached_account_from_value(profile.as_ref())
}

fn read_json(path: &PathBuf) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn cached_account_from_value(profile: Option<&Value>) -> Option<CachedClaudeAccount> {
    let oauth = profile?.get("oauthAccount")?;
    let email = string_field(oauth, &["emailAddress", "email"]);
    let account_id = string_field(oauth, &["accountUuid", "accountId"]);
    let display_name = string_field(oauth, &["fullName", "displayName"]);
    let organization_id = string_field(oauth, &["organizationUuid", "organizationId"]);
    let subscription = string_field(oauth, &["subscriptionType", "seatTier"]).or_else(|| {
        string_field(oauth, &["billingType"])
            .filter(|value| value.to_ascii_lowercase().contains("subscription"))
            .map(|_| "claude_subscription".into())
    });
    (email.is_some() || account_id.is_some()).then_some(CachedClaudeAccount {
        account_id,
        email,
        display_name,
        organization_id,
        subscription,
    })
}

fn capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        usage_percentage: true,
        token_usage: true,
        local_session_detection: true,
        oauth: true,
        multiple_accounts: true,
        app_launch: true,
        rate_limit_status: true,
        ..Default::default()
    }
}

fn string_field(value: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        value
            .get(*name)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
    })
}

fn format_plan(value: &str) -> String {
    value
        .split(['_', '-'])
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

fn query_auth_status() -> Result<Value> {
    let mut errors = Vec::new();
    for executable in claude_candidates() {
        let mut command = Command::new(&executable);
        command
            .args(["auth", "status"])
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        match command.output() {
            Ok(output) => match serde_json::from_slice::<Value>(&output.stdout) {
                Ok(value) => return Ok(value),
                Err(error) => errors.push(format!("{}: {error}", executable.display())),
            },
            Err(error) => errors.push(format!("{}: {error}", executable.display())),
        }
    }
    Err(HubError::Operation(format!(
        "Claude Code auth status is unavailable: {}",
        errors
            .last()
            .cloned()
            .unwrap_or_else(|| "Claude Code executable was not found".into())
    )))
}

fn claude_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("AI_USAGE_HUB_CLAUDE_EXECUTABLE") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(app_data) = env::var_os("APPDATA") {
        let root = PathBuf::from(app_data).join("Claude/claude-code");
        if let Ok(entries) = fs::read_dir(root) {
            let mut installed: Vec<_> = entries
                .flatten()
                .map(|entry| entry.path().join("claude.exe"))
                .filter(|path| path.is_file())
                .collect();
            installed.sort_by_key(|path| fs::metadata(path).and_then(|item| item.modified()).ok());
            installed.reverse();
            candidates.extend(installed);
        }
    }
    if let Some(user_profile) = env::var_os("USERPROFILE") {
        candidates.push(PathBuf::from(user_profile).join(".local/bin/claude.exe"));
    }
    candidates.push(PathBuf::from("claude"));
    candidates.dedup();
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_names_are_readable() {
        assert_eq!(format_plan("max_20x"), "Max 20x");
    }

    #[test]
    fn cached_profile_keeps_the_last_safe_account_fields() {
        let profile = serde_json::json!({
            "oauthAccount": {
                "accountUuid": "account-1",
                "emailAddress": "person@example.com",
                "fullName": "Person",
                "subscriptionType": "pro"
            }
        });
        let cached = cached_account_from_value(Some(&profile)).unwrap();
        assert_eq!(cached.email.as_deref(), Some("person@example.com"));
        assert_eq!(cached.subscription.as_deref(), Some("pro"));
    }

    #[test]
    fn desktop_history_matches_account_and_converts_used_to_remaining() {
        let usage = desktop_usage_from_bytes(
            br#"{"version":1,"samples":[{"t":1700000000000,"org":"old","u":{"fh":70,"sd":96}},{"t":1700000300000,"org":"active","u":{"fh":39,"sd":51}}]}"#,
            Some("active"),
        )
        .unwrap();
        let values = usage.usage_values();
        assert_eq!(usage.retrieved_at, 1_700_000_300);
        assert_eq!(values[0].value, Some(61.0));
        assert_eq!(values[1].value, Some(49.0));
        assert_eq!(values[0].source, UsageSource::LocalApplication);
    }

    #[test]
    fn infers_only_future_resets_from_matching_history() {
        let now = 1_800_000_000;
        let sample = |t, fh, sd| DesktopUsageSample {
            t,
            org: "active".into(),
            u: DesktopUsagePercent {
                fh: Some(fh),
                sd: Some(sd),
            },
        };
        let samples = vec![
            sample(now - 10 * 86_400, 90.0, 91.0),
            sample(now - 10 * 86_400 + 60, 0.0, 0.0),
            sample(now - 3 * 86_400, 90.0, 97.0),
            sample(now - 3 * 86_400 + 60, 0.0, 0.0),
            sample(now - 3_600 - 60, 80.0, 14.0),
            sample(now - 3_600, 0.0, 14.0),
        ];
        assert_eq!(
            inferred_resets(&samples, "active", now),
            (Some(now + 4 * 3_600), Some(now + 4 * 86_400 + 60))
        );
        assert_eq!(inferred_resets(&samples, "other", now), (None, None));
    }

    #[test]
    fn local_token_log_deduplicates_repeated_messages() {
        let data = br#"{"timestamp":"2026-09-18T00:00:01Z","message":{"id":"one","usage":{"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":30,"output_tokens":40}}}
{"timestamp":"2026-09-18T00:00:02Z","message":{"id":"one","usage":{"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":30,"output_tokens":45}}}
{"timestamp":"2026-09-18T00:00:03Z","message":{"id":"two","usage":{"input_tokens":1,"output_tokens":9}}}"#;
        let mut messages = HashMap::new();
        collect_claude_tokens(&data[..], 1_789_689_600, &mut messages);
        assert_eq!(
            messages.values().map(|(_, tokens)| tokens).sum::<i64>(),
            115
        );
    }

    #[test]
    #[ignore = "requires recent local Claude sessions"]
    fn live_local_token_totals_are_nonzero() {
        assert!(recent_claude_tokens(Utc::now().timestamp()) > 0);
    }

    #[test]
    #[ignore = "requires an installed Claude Code binary"]
    fn live_auth_status_round_trip() -> Result<()> {
        let value = query_auth_status()?;
        assert!(value.get("loggedIn").is_some());
        Ok(())
    }

    #[test]
    #[ignore = "requires a local Claude profile"]
    fn live_cached_account_round_trip() -> Result<()> {
        let summary = summary()?;
        assert_eq!(summary.id, "anthropic");
        assert!(summary.account_key.is_some());
        assert!(summary.usage.iter().any(|usage| usage.value.is_some()));
        Ok(())
    }
}
