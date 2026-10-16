//! Providers read with an API key the user pastes into the app: DeepSeek, Kimi (Moonshot) and
//! Grok (xAI). Each calls the provider's documented account endpoint with that key. Keys are
//! stored encrypted by the database layer and never leave this PC except to their own provider.

use std::time::Duration;

use chrono::Utc;
use reqwest::blocking::Client;
use serde_json::Value;

use crate::{
    error::{HubError, Result},
    models::{Confidence, ConnectionStatus, ProviderCapabilities, ProviderSummary, UsageSource, UsageValue},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

/// Provider ids that are connected by pasting an API key.
pub const KEYED_PROVIDERS: [&str; 3] = ["kimi", "grok", "deepseek"];

/// Display name of a key-based provider.
pub fn name(provider_id: &str) -> &'static str {
    match provider_id {
        "kimi" => "Kimi / Moonshot",
        "grok" => "Grok / xAI",
        _ => "DeepSeek",
    }
}

/// What a key-based provider card can show.
pub fn capabilities() -> ProviderCapabilities {
    ProviderCapabilities { cost_tracking: true, ..Default::default() }
}

/// Live summary for one key-based provider.
///
/// DeepSeek and Kimi report the account balance. xAI reports no balance for a regular API key,
/// so the Grok card only confirms that the key works and whether it is blocked. A rejected key
/// is an error naming the provider.
pub fn summary(provider_id: &str, key: &str) -> Result<ProviderSummary> {
    let client = Client::builder().timeout(REQUEST_TIMEOUT).build().map_err(operation)?;
    let now = Utc::now().timestamp();
    let mut usage_allowed = None;
    let (label, usage) = match provider_id {
        "deepseek" => {
            let body = get(&client, "https://api.deepseek.com/user/balance", key, "DeepSeek")?;
            usage_allowed = body.get("is_available").and_then(Value::as_bool);
            (masked(key), deepseek_usage(&body, now))
        }
        "kimi" => {
            // A key belongs to one of the two Moonshot platforms; the other one rejects it.
            let (body, currency) = get(&client, "https://api.moonshot.ai/v1/users/me/balance", key, "Kimi")
                .map(|body| (body, "USD"))
                .or_else(|_| get(&client, "https://api.moonshot.cn/v1/users/me/balance", key, "Kimi").map(|body| (body, "CNY")))?;
            (masked(key), kimi_usage(&body, currency, now))
        }
        "grok" => {
            let body = get(&client, "https://api.x.ai/v1/api-key", key, "xAI")?;
            let blocked = ["api_key_blocked", "api_key_disabled", "team_blocked"].iter().any(|flag| body.get(*flag).and_then(Value::as_bool) == Some(true));
            usage_allowed = Some(!blocked);
            (body.get("name").and_then(Value::as_str).filter(|name| !name.is_empty()).map_or_else(|| masked(key), str::to_owned), Vec::new())
        }
        other => return Err(operation(format!("{other} is not a key-based provider"))),
    };
    Ok(ProviderSummary {
        id: provider_id.into(),
        name: name(provider_id).into(),
        account_key: Some(format!("key:{}", key_tail(key))),
        account_label: label,
        plan: Some("API key".into()),
        connection_status: ConnectionStatus::Connected,
        capabilities: capabilities(),
        usage,
        usage_allowed,
        last_refresh: Some(now),
        error: None,
    })
}

fn key_tail(key: &str) -> String {
    let characters: Vec<char> = key.trim().chars().collect();
    characters[characters.len().saturating_sub(4)..].iter().collect()
}

/// A key shown as its last four characters only.
fn masked(key: &str) -> String {
    format!("API key ••••{}", key_tail(key))
}

fn get(client: &Client, url: &str, key: &str, provider: &str) -> Result<Value> {
    let response = client.get(url).bearer_auth(key.trim()).send().map_err(operation)?;
    match response.status().as_u16() {
        200..=299 => response.json().map_err(operation),
        401 | 403 => Err(operation(format!("{provider} rejected the API key; check it in Integrations"))),
        status => Err(operation(format!("{provider} account request failed ({status})"))),
    }
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.trim().parse().ok())
}

fn balance_row(metric: &str, label: &str, amount: f64, currency: &str, now: i64) -> UsageValue {
    UsageValue {
        metric: metric.into(),
        label: label.into(),
        value: Some(amount),
        max_value: None,
        unit: currency.into(),
        source: UsageSource::OfficialApi,
        confidence: Confidence::High,
        retrieved_at: now,
        reset_at: None,
    }
}

/// DeepSeek `GET /user/balance`: one total-balance row per currency the account holds.
fn deepseek_usage(body: &Value, now: i64) -> Vec<UsageValue> {
    body.get("balance_infos")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|info| {
            let currency = info.get("currency")?.as_str()?;
            let total = number(info.get("total_balance")?)?;
            Some(balance_row(&format!("deepseek_balance_{}", currency.to_ascii_lowercase()), "DeepSeek balance", total, currency, now))
        })
        .collect()
}

/// Moonshot `GET /v1/users/me/balance`: the spendable balance (cash plus vouchers).
fn kimi_usage(body: &Value, currency: &str, now: i64) -> Vec<UsageValue> {
    body.pointer("/data/available_balance")
        .and_then(number)
        .map(|balance| balance_row("kimi_balance", "Kimi balance", balance, currency, now))
        .into_iter()
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
    fn balances_are_read_from_documented_shapes() {
        let deepseek = json!({ "is_available": true, "balance_infos": [{ "currency": "USD", "total_balance": "12.50", "granted_balance": "0.00", "topped_up_balance": "12.50" }] });
        let rows = deepseek_usage(&deepseek, 0);
        assert_eq!((rows[0].value, rows[0].unit.as_str()), (Some(12.5), "USD"));

        let kimi = json!({ "code": 0, "data": { "available_balance": 49.58894, "voucher_balance": 46.58893, "cash_balance": 3.00001 }, "status": true });
        assert_eq!(kimi_usage(&kimi, "CNY", 0)[0].value, Some(49.58894));
        assert!(kimi_usage(&json!({ "error": {} }), "USD", 0).is_empty());
        assert!(deepseek_usage(&json!({}), 0).is_empty());
    }

    #[test]
    fn keys_are_only_ever_shown_masked() {
        assert_eq!(masked("sk-abcdef123456"), "API key ••••3456");
        assert_eq!(key_tail("ab"), "ab");
    }
}
