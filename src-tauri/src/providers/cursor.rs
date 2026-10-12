//! Cursor usage. Reads the signed-in session Cursor keeps in its own `state.vscdb` and asks the
//! usage summary its dashboard shows. The endpoint is not publicly documented and was written
//! from its known response shape without a Cursor install to test against, so rows carry medium
//! confidence and a response in another shape yields no rows instead of guessed ones.

use std::{path::PathBuf, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::{
    antigravity::base64_url_decode,
    error::{HubError, Result},
    models::{Confidence, ConnectionStatus, ProviderCapabilities, ProviderSummary, UsageSource, UsageValue},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const SIGN_IN_HINT: &str = "Open Cursor and sign in to read Cursor usage";

/// Live Cursor summary: signed-in e-mail, membership and the dollars of included and on-demand
/// usage spent in the current billing cycle. Fails with a sign-in hint when Cursor is not
/// installed or not signed in.
pub fn summary() -> Result<ProviderSummary> {
    let database = state_database().ok_or_else(|| operation(SIGN_IN_HINT))?;
    let item = |key: &str| -> Option<String> {
        database
            .query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| row.get::<_, String>(0))
            .ok()
            .filter(|value| !value.is_empty())
    };
    let token = item("cursorAuth/accessToken").ok_or_else(|| operation(SIGN_IN_HINT))?;
    let user_id = user_id_from_token(&token).ok_or_else(|| operation("Cursor's saved session could not be read; sign in again"))?;
    let email = item("cursorAuth/cachedEmail");
    let membership = item("cursorAuth/stripeMembershipType");

    let client = Client::builder().timeout(REQUEST_TIMEOUT).build().map_err(operation)?;
    let response = client
        .get("https://cursor.com/api/usage-summary")
        .header(reqwest::header::COOKIE, format!("WorkosCursorSessionToken={user_id}%3A%3A{token}"))
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .map_err(operation)?;
    if matches!(response.status().as_u16(), 401 | 403) {
        return Err(operation("Cursor sign-in has expired; open Cursor and sign in again"));
    }
    if !response.status().is_success() {
        return Err(operation(format!("Cursor rejected the usage request ({})", response.status().as_u16())));
    }
    let body: Value = response.json().map_err(operation)?;
    let now = Utc::now().timestamp();
    let usage = usage_from_summary(&body, now);
    let label = email.unwrap_or_else(|| "Cursor account".into());
    Ok(ProviderSummary {
        id: "cursor".into(),
        name: "Cursor".into(),
        account_key: Some(format!("email:{}", label.to_lowercase())),
        account_label: label,
        plan: body.get("membershipType").and_then(Value::as_str).map(str::to_owned).or(membership),
        connection_status: ConnectionStatus::Connected,
        capabilities: capabilities(),
        usage_allowed: None,
        usage,
        last_refresh: Some(now),
        error: None,
    })
}

/// What the Cursor card can show once a sign-in is found.
pub fn capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        reset_time: true,
        cost_tracking: true,
        local_session_detection: true,
        app_launch: true,
        ..Default::default()
    }
}

fn state_database() -> Option<Connection> {
    let path = PathBuf::from(std::env::var_os("APPDATA")?).join("Cursor/User/globalStorage/state.vscdb");
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
}

/// The user id inside the session token's `sub` claim (`provider|user_...`).
fn user_id_from_token(token: &str) -> Option<String> {
    let claims: Value = serde_json::from_slice(&base64_url_decode(token.split('.').nth(1)?)?).ok()?;
    let subject = claims.get("sub")?.as_str()?;
    Some(subject.rsplit('|').next()?.to_owned()).filter(|id| !id.is_empty())
}

/// Rows from a usage summary: amounts are reported in cents and shown in dollars. A spent amount
/// above its limit keeps the amount and drops the limit instead of being clamped.
fn usage_from_summary(body: &Value, now: i64) -> Vec<UsageValue> {
    let reset_at = body
        .get("billingCycleEnd")
        .and_then(Value::as_str)
        .and_then(|time| DateTime::parse_from_rfc3339(time).ok())
        .map(|time| time.timestamp());
    [("plan", "Cursor included usage"), ("onDemand", "Cursor on-demand usage")]
        .iter()
        .filter_map(|(key, label)| {
            let section = body.pointer(&format!("/individualUsage/{key}"))?;
            if section.get("enabled").and_then(Value::as_bool) == Some(false) {
                return None;
            }
            let used = section.get("used")?.as_f64()? / 100.0;
            let limit = section.get("limit").and_then(Value::as_f64).map(|limit| limit / 100.0).filter(|limit| *limit > 0.0 && used <= *limit);
            Some(UsageValue {
                metric: format!("cursor_{}_used", key.to_ascii_lowercase()),
                label: (*label).to_owned(),
                value: Some(used.max(0.0)),
                max_value: limit,
                unit: "USD".into(),
                source: UsageSource::OfficialApi,
                confidence: Confidence::Medium,
                retrieved_at: now,
                reset_at,
            })
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
    fn user_id_comes_from_the_token_subject() {
        // payload {"sub":"auth0|user_42"}
        assert_eq!(user_id_from_token("h.eyJzdWIiOiJhdXRoMHx1c2VyXzQyIn0.s").as_deref(), Some("user_42"));
        assert_eq!(user_id_from_token("not-a-token"), None);
    }

    #[test]
    fn summary_cents_become_dollar_rows() {
        let body = json!({ "billingCycleEnd": "2030-01-01T00:00:00Z", "individualUsage": {
            "plan": { "enabled": true, "used": 1250, "limit": 2000 },
            "onDemand": { "enabled": false, "used": 0, "limit": null }
        }});
        let rows = usage_from_summary(&body, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].value, rows[0].max_value, rows[0].reset_at), (Some(12.5), Some(20.0), Some(1_893_456_000)));
        assert!(rows[0].validate().is_ok());
        assert!(usage_from_summary(&json!({ "unexpected": true }), 0).is_empty());
    }
}
