use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoogleAccountInput {
    pub id: Option<String>,
    pub email: String,
    pub name: String,
    pub color: String,
    pub subscription: String,
    pub activation_date: Option<String>,
    pub expiration_date: Option<String>,
    pub enabled: bool,
    pub archived: bool,
    pub favorite: bool,
    pub group: String,
    pub tags: Vec<String>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleAccount {
    #[serde(flatten)]
    pub metadata: GoogleAccountInput,
    pub antigravity_id: Option<String>,
    pub last_used: Option<i64>,
    pub created_at: i64,
    pub quota_remaining: Option<f64>,
    /// Local observation time of cached native quota; never a provider refresh timestamp.
    pub quota_updated_at: Option<i64>,
    pub quota_details: Option<String>,
}

impl std::ops::Deref for GoogleAccount {
    type Target = GoogleAccountInput;
    fn deref(&self) -> &Self::Target {
        &self.metadata
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoogleAssignment {
    pub account_id: String,
    pub role: String,
    pub services: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoogleProjectInput {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub notes: String,
    pub status: String,
    pub assignments: Vec<GoogleAssignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleProject {
    #[serde(flatten)]
    pub metadata: GoogleProjectInput,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoogleUsageInput {
    pub account_id: String,
    pub service_id: String,
    pub metric: String,
    pub value: f64,
    pub max_value: Option<f64>,
    pub unit: String,
    pub recorded_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleUsageRecord {
    pub id: i64,
    pub account_id: String,
    pub service_id: String,
    pub metric: String,
    pub value: f64,
    pub max_value: Option<f64>,
    pub unit: String,
    pub recorded_at: i64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleServiceStatus {
    pub account_id: String,
    pub status: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleService {
    pub id: String,
    pub name: String,
    pub url: String,
    pub favorite: bool,
    pub last_account_id: Option<String>,
    pub statuses: Vec<GoogleServiceStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleActivity {
    pub id: i64,
    pub kind: String,
    pub account_id: Option<String>,
    pub project_id: Option<String>,
    pub title: String,
    pub detail: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GooglePreferences {
    pub notifications_enabled: bool,
    pub expiration_warning_days: i64,
    pub auto_backup: bool,
    pub backup_retention: i64,
}

impl Default for GooglePreferences {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            expiration_warning_days: 7,
            auto_backup: true,
            backup_retention: 7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleBackup {
    pub name: String,
    pub path: String,
    pub created_at: i64,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleSnapshot {
    pub accounts: Vec<GoogleAccount>,
    pub projects: Vec<GoogleProject>,
    pub usage: Vec<GoogleUsageRecord>,
    pub services: Vec<GoogleService>,
    pub activity: Vec<GoogleActivity>,
    pub preferences: GooglePreferences,
    pub backups: Vec<GoogleBackup>,
}
