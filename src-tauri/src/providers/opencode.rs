use std::{env, fs, path::PathBuf, time::Duration};

use chrono::Utc;
use reqwest::blocking::Client;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Deserialize;
use serde_json::Value;
use sysinfo::{ProcessesToUpdate, System};

use crate::{
    error::{HubError, Result},
    models::{
        Confidence, ConnectionStatus, ProviderCapabilities, ProviderSummary, UsageSource,
        UsageValue,
    },
};

const MODEL_CATALOG: &str = "https://opencode.ai/zen/v1/models";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Default)]
struct LocalUsage {
    sessions: i64,
    tokens: i64,
    cost: f64,
    latest_provider: Option<String>,
    active_email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelCatalog {
    #[serde(default)]
    data: Vec<CatalogModel>,
}

#[derive(Debug, Deserialize)]
struct CatalogModel {
    id: String,
}

pub fn summary() -> Result<ProviderSummary> {
    let now = Utc::now().timestamp();
    let root = data_root();
    let provider_ids = root
        .as_ref()
        .map(connected_provider_ids)
        .transpose()?
        .unwrap_or_default();
    let local = root
        .as_ref()
        .map(read_local_usage)
        .transpose()?
        .unwrap_or_default();
    let free_models = fetch_free_models().unwrap_or_default();
    let connected = is_connected(&provider_ids, opencode_running());

    let mut usage = Vec::new();
    if !free_models.is_empty() {
        usage.push(UsageValue {
            metric: "opencode_free_models".into(),
            label: "OpenCode free model catalog".into(),
            value: Some(free_models.len() as f64),
            max_value: None,
            unit: "models".into(),
            source: UsageSource::OfficialApi,
            confidence: Confidence::High,
            retrieved_at: now,
            reset_at: None,
        });
    }

    if local.tokens > 0 {
        usage.push(UsageValue {
            metric: "opencode_local_tokens".into(),
            label: "Locally tracked model tokens".into(),
            value: Some(local.tokens as f64),
            max_value: None,
            unit: "tokens".into(),
            source: UsageSource::LocalApplication,
            confidence: Confidence::High,
            retrieved_at: now,
            reset_at: None,
        });
    }
    if local.sessions > 0 {
        usage.push(UsageValue {
            metric: "opencode_local_sessions".into(),
            label: "Locally tracked OpenCode sessions".into(),
            value: Some(local.sessions as f64),
            max_value: None,
            unit: "sessions".into(),
            source: UsageSource::LocalApplication,
            confidence: Confidence::High,
            retrieved_at: now,
            reset_at: None,
        });
    }
    if local.cost > 0.0 {
        usage.push(UsageValue {
            metric: "opencode_local_cost".into(),
            label: "Locally tracked provider cost".into(),
            value: Some(local.cost),
            max_value: None,
            unit: "USD".into(),
            source: UsageSource::LocalApplication,
            confidence: Confidence::High,
            retrieved_at: now,
            reset_at: None,
        });
    }
    if usage.is_empty() {
        usage.push(UsageValue::unavailable(
            "opencode_usage",
            "OpenCode usage and model catalog",
        ));
    }

    let account_label = if connected {
        local.active_email.clone()
    } else {
        None
    }
    .unwrap_or_else(|| {
        if !connected {
            "OpenCode Desktop".into()
        } else {
            format!(
                "OpenCode Desktop · {}",
                provider_ids
                    .iter()
                    .map(|value| readable_provider(value))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    });
    let mut plan_parts = Vec::new();
    if connected {
        if let Some(provider) = local.latest_provider.as_deref() {
            plan_parts.push(format!("Last used: {}", readable_provider(provider)));
        }
    }
    if !free_models.is_empty() {
        plan_parts.push(format!("{} free OpenCode models", free_models.len()));
    }

    Ok(ProviderSummary {
        id: "opencode".into(),
        name: "OpenCode".into(),
        account_key: connected.then(|| {
            local
                .active_email
                .as_deref()
                .map(|email| format!("email:{}", email.to_lowercase()))
                .unwrap_or_else(|| "desktop".into())
        }),
        account_label,
        plan: (!plan_parts.is_empty()).then(|| plan_parts.join(" · ")),
        connection_status: if connected {
            ConnectionStatus::Connected
        } else {
            ConnectionStatus::NotConnected
        },
        capabilities: ProviderCapabilities {
            request_count: true,
            token_usage: true,
            cost_tracking: true,
            local_session_detection: true,
            multiple_accounts: true,
            app_launch: true,
            rate_limit_status: true,
            ..Default::default()
        },
        usage,
        usage_allowed: None,
        last_refresh: Some(now),
        error: None,
    })
}

fn data_root() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("OPENCODE_DATA_HOME") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(path) = env::var_os("XDG_DATA_HOME") {
        candidates.push(PathBuf::from(path).join("opencode"));
    }
    if let Some(home) = crate::platform::home_dir() {
        candidates.push(home.join(".local/share/opencode"));
    }
    candidates.into_iter().find(|path| path.is_dir())
}

fn connected_provider_ids(root: &PathBuf) -> Result<Vec<String>> {
    let path = root.join("auth.json");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| HubError::Configuration(error.to_string()))?;
    let mut ids = value
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    ids.sort();
    Ok(ids)
}

fn read_local_usage(root: &PathBuf) -> Result<LocalUsage> {
    let path = root.join("opencode.db");
    if !path.is_file() {
        return Ok(LocalUsage::default());
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let (sessions, tokens, cost): (i64, i64, f64) = connection.query_row(
        "SELECT COUNT(*), COALESCE(SUM(tokens_input + tokens_output + tokens_reasoning), 0), COALESCE(SUM(cost), 0) FROM session",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let latest = connection
        .query_row(
            "SELECT model FROM session WHERE model IS NOT NULL ORDER BY time_updated DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|value| parse_model_reference(&value));
    let active_email = connection
        .query_row(
            "SELECT a.email FROM account a JOIN account_state s ON s.active_account_id = a.id LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .unwrap_or(None);

    Ok(LocalUsage {
        sessions,
        tokens,
        cost,
        latest_provider: latest.as_ref().map(|(provider, _)| provider.clone()),
        active_email,
    })
}

fn parse_model_reference(value: &str) -> Option<(String, String)> {
    let value: Value = serde_json::from_str(value).ok()?;
    Some((
        value.get("providerID")?.as_str()?.to_owned(),
        value
            .get("id")
            .or_else(|| value.get("modelID"))?
            .as_str()?
            .to_owned(),
    ))
}

fn is_connected(provider_ids: &[String], app_running: bool) -> bool {
    app_running && !provider_ids.is_empty()
}

fn opencode_running() -> bool {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    system.processes().values().any(|process| {
        let name = process.name().to_string_lossy();
        name.eq_ignore_ascii_case("opencode.exe") || name.eq_ignore_ascii_case("opencode")
    })
}

fn fetch_free_models() -> Result<Vec<String>> {
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent("AI-Usage-Hub/0.1")
        .build()
        .map_err(operation)?;
    let catalog: ModelCatalog = client
        .get(MODEL_CATALOG)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(operation)?
        .json()
        .map_err(operation)?;
    Ok(free_model_ids(catalog))
}

fn free_model_ids(catalog: ModelCatalog) -> Vec<String> {
    let mut models = catalog
        .data
        .into_iter()
        .filter(|model| model.id.ends_with("-free"))
        .map(|model| model.id)
        .collect::<Vec<_>>();
    models.sort();
    models
}

fn readable_provider(value: &str) -> String {
    match value {
        "nvidia" => "NVIDIA".into(),
        "opencode" => "OpenCode Console".into(),
        "opencode-go" => "OpenCode Go".into(),
        other => other
            .split(['-', '_'])
            .map(|part| {
                let mut chars = part.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn operation(error: reqwest::Error) -> HubError {
    HubError::Operation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_provider_and_model_without_confusing_them() {
        let value = r#"{"id":"moonshotai/kimi-k3","providerID":"nvidia","variant":"default"}"#;
        assert_eq!(
            parse_model_reference(value),
            Some(("nvidia".into(), "moonshotai/kimi-k3".into()))
        );
    }

    #[test]
    fn only_catalog_models_explicitly_marked_free_are_counted() {
        let catalog: ModelCatalog = serde_json::from_str(
            r#"{"data":[{"id":"mimo-v2.5-free"},{"id":"gpt-5.6-sol"},{"id":"nemotron-free"}]}"#,
        )
        .unwrap();
        assert_eq!(
            free_model_ids(catalog),
            vec!["mimo-v2.5-free", "nemotron-free"]
        );
    }

    #[test]
    fn saved_credentials_do_not_mean_an_active_connection() {
        assert!(!is_connected(&["nvidia".into()], false));
        assert!(!is_connected(&[], true));
        assert!(is_connected(&["nvidia".into()], true));
    }

    #[test]
    #[ignore = "requires an installed OpenCode app and network access"]
    fn live_opencode_summary_round_trip() -> Result<()> {
        let summary = summary()?;
        assert_eq!(summary.id, "opencode");
        assert!(summary.usage.iter().any(|usage| usage.value.is_some()));
        Ok(())
    }
}
