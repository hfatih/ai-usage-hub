use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunningState {
    Running,
    NotRunning,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    OfficialApi,
    OfficialCli,
    LocalApplication,
    LocalLogs,
    LocallyCalculated,
    Estimated,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageValue {
    pub metric: String,
    pub label: String,
    pub value: Option<f64>,
    pub max_value: Option<f64>,
    pub unit: String,
    pub source: UsageSource,
    pub confidence: Confidence,
    pub retrieved_at: i64,
    pub reset_at: Option<i64>,
}

impl UsageValue {
    pub fn unavailable(metric: &str, label: &str) -> Self {
        Self {
            metric: metric.into(),
            label: label.into(),
            value: None,
            max_value: None,
            unit: String::new(),
            source: UsageSource::Unavailable,
            confidence: Confidence::Unavailable,
            retrieved_at: chrono::Utc::now().timestamp(),
            reset_at: None,
        }
    }

    pub fn validate(&self) -> std::result::Result<(), &'static str> {
        if self.source == UsageSource::Unavailable && self.value.is_some() {
            return Err("unavailable usage cannot contain a value");
        }
        if let (Some(value), Some(maximum)) = (self.value, self.max_value) {
            if value < 0.0 || maximum <= 0.0 || value > maximum {
                return Err("usage value must be within its declared maximum");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub usage_percentage: bool,
    pub reset_time: bool,
    pub request_count: bool,
    pub token_usage: bool,
    pub cost_tracking: bool,
    pub local_session_detection: bool,
    pub oauth: bool,
    pub multiple_accounts: bool,
    pub app_launch: bool,
    pub rate_limit_status: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Connected,
    NotConnected,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub account_key: Option<String>,
    pub account_label: String,
    pub plan: Option<String>,
    pub connection_status: ConnectionStatus,
    pub capabilities: ProviderCapabilities,
    pub usage: Vec<UsageValue>,
    pub usage_allowed: Option<bool>,
    pub last_refresh: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ApplicationDefinition {
    pub id: String,
    pub provider_id: Option<String>,
    pub display_name: String,
    pub executable_path: Option<String>,
    pub launch_arguments: Vec<String>,
    pub process_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub id: String,
    pub provider_id: Option<String>,
    pub display_name: String,
    pub state: RunningState,
    pub detected_executable: Option<String>,
    pub executable_path: Option<String>,
    pub running_since: Option<i64>,
    pub current_session_seconds: i64,
    pub today_seconds: i64,
    pub week_seconds: i64,
    pub last_used_at: Option<i64>,
    pub launchable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub providers: Vec<ProviderSummary>,
    pub applications: Vec<AppStatus>,
    pub refreshed_at: i64,
    pub monitoring_paused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Success,
    Warning,
    Danger,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    pub id: i64,
    pub event_type: String,
    pub title: String,
    pub detail: Option<String>,
    pub severity: Severity,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub email: String,
    pub plan: Option<String>,
    pub auth_method: String,
    pub connection_status: ConnectionStatus,
    pub last_refresh: Option<i64>,
    pub usage: Vec<UsageValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageHistoryPoint {
    pub id: i64,
    pub account_id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub account_label: String,
    pub usage: UsageValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityStatus {
    pub database_path: String,
    pub database_size_bytes: u64,
    pub account_count: i64,
    pub usage_snapshot_count: i64,
    pub stored_secret_count: i64,
    pub provider_credentials_stored: bool,
    pub secrets_encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityQuotaDetails {
    pub gemini_5h: Option<f64>,
    pub gemini_weekly: Option<f64>,
    pub gemini_reset_at: Option<i64>,
    pub claude_5h: Option<f64>,
    pub claude_weekly: Option<f64>,
    pub claude_reset_at: Option<i64>,
    pub gemini_weekly_reset_at: Option<i64>,
    pub claude_weekly_reset_at: Option<i64>,
    /// Gemini 4 Argon weekly allowance left, in percent. Only the running Antigravity app reports
    /// it (Google's quota endpoints do not), so it is filled while the account is the active one
    /// and kept afterwards; `argon_read_at` says how old it is.
    #[serde(default)]
    pub argon_weekly: Option<f64>,
    #[serde(default)]
    pub argon_weekly_reset_at: Option<i64>,
    #[serde(default)]
    pub argon_read_at: Option<i64>,
}

impl AntigravityQuotaDetails {
    /// Copies the Argon reading from `previous` when this (fresher) reading has none, so a
    /// Google-side refresh does not erase what the app reported earlier.
    pub fn keep_argon_from(&mut self, previous: Option<&AntigravityQuotaDetails>) {
        if self.argon_weekly.is_none() {
            if let Some(previous) = previous {
                self.argon_weekly = previous.argon_weekly;
                self.argon_weekly_reset_at = previous.argon_weekly_reset_at;
                self.argon_read_at = previous.argon_read_at;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityAccount {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: Option<String>,
    pub project_id: Option<String>,
    pub expiry_timestamp: i64,
    pub status: String,
    pub is_current: bool,
    pub last_used: i64,
    pub quota_remaining_percent: Option<f64>,
    #[serde(default)]
    pub quota_details: Option<AntigravityQuotaDetails>,
    /// Optional `http://`, `https://` or `socks5://` proxy every Google call for this account goes
    /// through. Requests fail rather than fall back to a direct connection when it is unreachable.
    #[serde(default)]
    pub proxy_url: Option<String>,
    /// Subscription tier id Google reports for the account (e.g. `g1-ultra-tier`), if known.
    #[serde(default)]
    pub tier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityLiveSession {
    pub is_running: bool,
    pub email: Option<String>,
    pub name: Option<String>,
    pub plan: Option<String>,
    pub source: String,
    pub port: Option<u16>,
    pub verified_at: i64,
    pub quota_details: Option<AntigravityQuotaDetails>,
    pub quota_remaining_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub launch_at_startup: bool,
    pub minimize_to_tray: bool,
    pub close_behavior: String,
    pub theme: String,
    pub language: String,
    pub process_monitoring: bool,
    pub refresh_interval_seconds: u64,
    pub notifications_enabled: bool,
    pub notify_warning_percent: u8,
    pub notify_critical_percent: u8,
    pub notify_on_reset: bool,
    pub notify_on_provider_error: bool,
    pub antigravity_auto_switch: bool,
    pub antigravity_switch_threshold: u8,
    pub antigravity_switch_on_rate_limit: bool,
    pub antigravity_switch_strategy: String,
    pub desktop_notifications: bool,
    pub proxy_api_key: String,
    pub proxy_require_key: bool,
    pub proxy_model_map: Vec<ModelMapRule>,
    /// Port the local proxy listens on; read once at startup.
    pub proxy_port: u16,
    /// Seconds the proxy waits for an upstream answer before giving up.
    pub proxy_timeout_seconds: u64,
    /// The proxy answers 403 until the user has accepted the terms-of-service warning. Settings
    /// saved before this field existed count as accepted so a working setup is not interrupted.
    #[serde(default = "acknowledged")]
    pub proxy_risk_acknowledged: bool,
    /// Send short housekeeping requests (conversation titles and the like) to a Flash model.
    pub proxy_background_downgrade: bool,
}

fn acknowledged() -> bool {
    true
}

/// A user rule sending one client-side model name to a chosen upstream model. `from` is matched
/// case-insensitively and may end with `*` to match a prefix, or start with `re:` to be read as a
/// regular expression.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelMapRule {
    pub from: String,
    pub to: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            launch_at_startup: false,
            minimize_to_tray: true,
            close_behavior: "tray".into(),
            theme: "dark".into(),
            language: "en".into(),
            process_monitoring: true,
            refresh_interval_seconds: 5,
            notifications_enabled: true,
            notify_warning_percent: 20,
            notify_critical_percent: 10,
            notify_on_reset: true,
            notify_on_provider_error: true,
            antigravity_auto_switch: false,
            antigravity_switch_threshold: 5,
            antigravity_switch_on_rate_limit: true,
            antigravity_switch_strategy: "highest_quota".into(),
            desktop_notifications: true,
            proxy_api_key: String::new(),
            proxy_require_key: true,
            proxy_model_map: Vec::new(),
            proxy_port: 8046,
            proxy_timeout_seconds: 90,
            proxy_risk_acknowledged: false,
            proxy_background_downgrade: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_usage_cannot_claim_a_value() {
        let mut value = UsageValue::unavailable("five_hour", "5-hour window");
        value.value = Some(72.0);
        assert!(value.validate().is_err());
    }

    #[test]
    fn usage_must_fit_the_declared_limit() {
        let value = UsageValue {
            metric: "rpm".into(),
            label: "Requests per minute".into(),
            value: Some(41.0),
            max_value: Some(40.0),
            unit: "requests".into(),
            source: UsageSource::OfficialApi,
            confidence: Confidence::High,
            retrieved_at: 0,
            reset_at: None,
        };
        assert!(value.validate().is_err());
    }
}
