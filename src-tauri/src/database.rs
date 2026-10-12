use std::path::{Path, PathBuf};

use chrono::{Datelike, Duration, Local, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{
    error::Result,
    models::{
        AccountSummary, ActivityEvent, AntigravityAccount, AppSettings, AppStatus, ApplicationDefinition, Confidence,
        ConnectionStatus, ProviderSummary, RunningState, SecurityStatus, Severity,
        UsageHistoryPoint, UsageSource, UsageValue,
    },
};

/// Matches pool accounts that still hold a sign-in token in plain text.
const PLAIN_TOKEN_FILTER: &str = "(access_token <> '' AND access_token NOT LIKE 'dpapi:%') OR (refresh_token <> '' AND refresh_token NOT LIKE 'dpapi:%') OR (COALESCE(id_token, '') <> '' AND id_token NOT LIKE 'dpapi:%')";

pub struct Database {
    connection: Connection,
    path: Option<PathBuf>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)?;
        let mut database = Self {
            connection,
            path: Some(path.to_path_buf()),
        };
        database.migrate()?;
        database.seed()?;
        Ok(database)
    }

    #[cfg(test)]
    pub(crate) fn memory() -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        let mut database = Self {
            connection,
            path: None,
        };
        database.migrate()?;
        database.seed()?;
        Ok(database)
    }

    fn migrate(&mut self) -> Result<()> {
        self.connection.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS providers (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE IF NOT EXISTS accounts (
                id TEXT PRIMARY KEY,
                provider_id TEXT NOT NULL REFERENCES providers(id),
                display_name TEXT NOT NULL,
                email_hint TEXT,
                plan TEXT,
                avatar TEXT,
                auth_method TEXT NOT NULL,
                connection_status TEXT NOT NULL,
                last_refresh INTEGER,
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE IF NOT EXISTS applications (
                id TEXT PRIMARY KEY,
                provider_id TEXT REFERENCES providers(id),
                display_name TEXT NOT NULL,
                executable_path TEXT,
                launch_arguments_json TEXT NOT NULL DEFAULT '[]',
                process_names_json TEXT NOT NULL,
                icon_path TEXT,
                enabled INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS app_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                app_id TEXT NOT NULL REFERENCES applications(id),
                account_id TEXT REFERENCES accounts(id),
                started_at INTEGER NOT NULL,
                ended_at INTEGER,
                duration_seconds INTEGER,
                detected_executable TEXT,
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE UNIQUE INDEX IF NOT EXISTS one_open_session_per_app
                ON app_sessions(app_id) WHERE ended_at IS NULL;
            CREATE INDEX IF NOT EXISTS app_sessions_time ON app_sessions(app_id, started_at);
            CREATE TABLE IF NOT EXISTS usage_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id TEXT NOT NULL REFERENCES accounts(id),
                metric TEXT NOT NULL,
                value REAL,
                max_value REAL,
                unit TEXT NOT NULL,
                source TEXT NOT NULL,
                confidence TEXT NOT NULL,
                window_type TEXT,
                window_started_at INTEGER,
                reset_at INTEGER,
                retrieved_at INTEGER NOT NULL,
                raw_metadata_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE IF NOT EXISTS rate_limit_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id TEXT NOT NULL REFERENCES accounts(id),
                metric TEXT NOT NULL,
                limit_value REAL,
                remaining_value REAL,
                reset_at INTEGER,
                retrieved_at INTEGER NOT NULL,
                source TEXT NOT NULL,
                raw_metadata_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE IF NOT EXISTS activity_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                app_id TEXT REFERENCES applications(id),
                account_id TEXT REFERENCES accounts(id),
                title TEXT NOT NULL,
                detail TEXT,
                severity TEXT NOT NULL DEFAULT 'info',
                created_at INTEGER NOT NULL,
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE INDEX IF NOT EXISTS activity_events_time ON activity_events(created_at DESC);
            CREATE TABLE IF NOT EXISTS notification_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id TEXT REFERENCES accounts(id),
                rule_type TEXT NOT NULL,
                threshold REAL,
                enabled INTEGER NOT NULL DEFAULT 1,
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS integrations (
                id TEXT PRIMARY KEY,
                provider_id TEXT REFERENCES providers(id),
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE IF NOT EXISTS antigravity_accounts (
                id TEXT PRIMARY KEY,
                email TEXT NOT NULL,
                display_name TEXT,
                access_token TEXT NOT NULL,
                refresh_token TEXT NOT NULL,
                id_token TEXT,
                project_id TEXT,
                expiry_timestamp INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'active',
                is_current INTEGER NOT NULL DEFAULT 0,
                last_used INTEGER NOT NULL,
                quota_remaining_percent REAL,
                quota_details_json TEXT
            );
            "#,
        )?;
        let _ = self.connection.execute("ALTER TABLE antigravity_accounts ADD COLUMN quota_details_json TEXT;", []);
        self.encrypt_legacy_tokens()?;
        // Rows written before 0.1.19 that were never real readings: the image-generation row
        // copied the Gemini percentage and the credit row held the usage threshold.
        self.connection.execute(
            "DELETE FROM usage_snapshots WHERE metric = 'antigravity_image_generation' OR metric LIKE 'google\\_credit\\_%' ESCAPE '\\'",
            [],
        )?;
        Ok(())
    }

    /// Rewrites sign-in tokens saved as plain text by versions before 0.1.22 in protected form.
    fn encrypt_legacy_tokens(&self) -> Result<()> {
        let legacy: Vec<(String, String, String, Option<String>)> = self
            .connection
            .prepare(&format!("SELECT id, access_token, refresh_token, id_token FROM antigravity_accounts WHERE {PLAIN_TOKEN_FILTER}"))?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for (id, access_token, refresh_token, id_token) in legacy {
            self.connection.execute(
                "UPDATE antigravity_accounts SET access_token = ?2, refresh_token = ?3, id_token = ?4 WHERE id = ?1",
                params![
                    id,
                    crate::secret::protect(&access_token),
                    crate::secret::protect(&refresh_token),
                    id_token.as_deref().map(crate::secret::protect),
                ],
            )?;
        }
        Ok(())
    }

    fn seed(&mut self) -> Result<()> {
        let tx = self.connection.transaction()?;
        for (id, name) in [
            ("openai", "ChatGPT / Codex"),
            ("anthropic", "Anthropic"),
            ("google", "Google / Antigravity"),
            ("opencode", "OpenCode"),
            ("kimi", "Kimi"),
            ("github", "GitHub"),
        ] {
            tx.execute(
                "INSERT OR IGNORE INTO providers (id, display_name) VALUES (?1, ?2)",
                params![id, name],
            )?;
        }

        for app in seed_applications() {
            tx.execute(
                "INSERT OR IGNORE INTO applications (id, provider_id, display_name, process_names_json) VALUES (?1, ?2, ?3, ?4)",
                params![app.id, app.provider_id, app.display_name, serde_json::to_string(&app.process_names).unwrap_or_else(|_| "[]".into())],
            )?;
        }
        tx.execute(
            "UPDATE applications SET provider_id = 'opencode' WHERE id = 'opencode'",
            [],
        )?;
        tx.execute("UPDATE providers SET enabled = 0 WHERE id = 'nvidia'", [])?;
        tx.commit()?;
        Ok(())
    }

    pub fn applications(&self) -> Result<Vec<ApplicationDefinition>> {
        let mut statement = self.connection.prepare(
            "SELECT id, provider_id, display_name, executable_path, launch_arguments_json, process_names_json FROM applications WHERE enabled = 1 ORDER BY rowid",
        )?;
        let rows = statement.query_map([], |row| {
            let args: String = row.get(4)?;
            let names: String = row.get(5)?;
            Ok(ApplicationDefinition {
                id: row.get(0)?,
                provider_id: row.get(1)?,
                display_name: row.get(2)?,
                executable_path: row.get(3)?,
                launch_arguments: serde_json::from_str(&args).unwrap_or_default(),
                process_names: serde_json::from_str(&names).unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn application(&self, id: &str) -> Result<Option<ApplicationDefinition>> {
        self.applications()
            .map(|apps| apps.into_iter().find(|app| app.id == id))
    }

    pub fn set_executable(&self, app_id: &str, path: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE applications SET executable_path = ?1 WHERE id = ?2",
            params![path, app_id],
        )?;
        Ok(())
    }

    pub fn sync_sessions(&mut self, statuses: &mut [AppStatus], now: i64) -> Result<bool> {
        let tx = self.connection.transaction()?;
        let mut changed = false;
        for status in statuses.iter_mut() {
            let active = tx
                .query_row(
                    "SELECT id, started_at FROM app_sessions WHERE app_id = ?1 AND ended_at IS NULL LIMIT 1",
                    [&status.id],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?;

            match (&status.state, active) {
                (RunningState::Running, None) => {
                    tx.execute(
                        "INSERT INTO app_sessions (app_id, started_at, detected_executable) VALUES (?1, ?2, ?3)",
                        params![status.id, now, status.detected_executable],
                    )?;
                    insert_activity(
                        &tx,
                        "app_started",
                        &status.id,
                        &format!("{} launched", status.display_name),
                        None,
                        "success",
                        now,
                    )?;
                    status.running_since = Some(now);
                    changed = true;
                }
                (RunningState::NotRunning, Some((session_id, started_at))) => {
                    let duration = (now - started_at).max(0);
                    tx.execute(
                        "UPDATE app_sessions SET ended_at = ?1, duration_seconds = ?2 WHERE id = ?3",
                        params![now, duration, session_id],
                    )?;
                    insert_activity(
                        &tx,
                        "app_stopped",
                        &status.id,
                        &format!("{} closed", status.display_name),
                        Some(&format!("Application-open time: {} minutes", duration / 60)),
                        "info",
                        now,
                    )?;
                    changed = true;
                }
                (RunningState::Running, Some((_, started_at))) => {
                    status.running_since = Some(started_at)
                }
                _ => {}
            }

            enrich_status(&tx, status, now)?;
        }
        tx.commit()?;
        Ok(changed)
    }

    pub fn activity(&self, limit: usize) -> Result<Vec<ActivityEvent>> {
        let mut statement = self.connection.prepare(
            "SELECT id, event_type, title, detail, severity, created_at FROM activity_events ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit as i64], |row| {
            let severity: String = row.get(4)?;
            Ok(ActivityEvent {
                id: row.get(0)?,
                event_type: row.get(1)?,
                title: row.get(2)?,
                detail: row.get(3)?,
                severity: match severity.as_str() {
                    "success" => Severity::Success,
                    "warning" => Severity::Warning,
                    "danger" => Severity::Danger,
                    _ => Severity::Info,
                },
                created_at: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn settings(&self) -> Result<AppSettings> {
        let json = self
            .connection
            .query_row(
                "SELECT value_json FROM settings WHERE key = 'app'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(json
            .and_then(|value| serde_json::from_str(&value).ok())
            .unwrap_or_default())
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        let json = serde_json::to_string(settings)
            .map_err(|error| crate::error::HubError::Configuration(error.to_string()))?;
        self.connection.execute(
            "INSERT INTO settings (key, value_json, updated_at) VALUES ('app', ?1, ?2) ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
            params![json, Utc::now().timestamp()],
        )?;
        Ok(())
    }

    pub fn save_provider_snapshot(&mut self, provider: &ProviderSummary) -> Result<()> {
        let settings = self.settings()?;
        let tx = self.connection.transaction()?;
        tx.execute(
            "UPDATE accounts SET connection_status = 'not_connected' WHERE provider_id = ?1",
            [&provider.id],
        )?;

        let Some(account_key) = provider.account_key.as_deref() else {
            tx.commit()?;
            return Ok(());
        };
        let account_id = format!("{}:{account_key}", provider.id);
        let auth_method = if provider.id == "google" {
            "official_application"
        } else {
            "official_cli"
        };
        tx.execute(
            "INSERT INTO accounts (id, provider_id, display_name, email_hint, plan, auth_method, connection_status, last_refresh) VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name, email_hint = excluded.email_hint, plan = excluded.plan, auth_method = excluded.auth_method, connection_status = excluded.connection_status, last_refresh = excluded.last_refresh",
            params![
                account_id,
                provider.id,
                provider.account_label,
                provider.plan,
                auth_method,
                connection_status_name(&provider.connection_status),
                provider.last_refresh,
            ],
        )?;
        // ponytail: transcript totals are device-wide; persist them per account only if logs expose reliable account IDs.
        for usage in provider
            .usage
            .iter()
            .filter(|usage| usage.value.is_some() && usage.source != UsageSource::LocalLogs)
        {
            let previous: Option<f64> = tx
                .query_row(
                    "SELECT value FROM usage_snapshots WHERE account_id = ?1 AND metric = ?2 AND value IS NOT NULL ORDER BY retrieved_at DESC, id DESC LIMIT 1",
                    params![account_id, usage.metric],
                    |row| row.get(0),
                )
                .optional()?;
            tx.execute(
                "INSERT INTO usage_snapshots (account_id, metric, value, max_value, unit, source, confidence, window_type, reset_at, retrieved_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    account_id,
                    usage.metric,
                    usage.value,
                    usage.max_value,
                    usage.unit,
                    usage_source_name(&usage.source),
                    confidence_name(&usage.confidence),
                    usage.label,
                    usage.reset_at,
                    usage.retrieved_at,
                ],
            )?;
            record_usage_alert(&tx, &account_id, provider, usage, previous, &settings)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn accounts(&self) -> Result<Vec<AccountSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT a.id, a.provider_id, p.display_name, a.email_hint, a.plan, a.auth_method, a.connection_status, a.last_refresh FROM accounts a JOIN providers p ON p.id = a.provider_id ORDER BY CASE a.connection_status WHEN 'connected' THEN 0 ELSE 1 END, COALESCE(a.last_refresh, 0) DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?
                    .unwrap_or_else(|| "Unknown account".into()),
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<i64>>(7)?,
            ))
        })?;
        let raw = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(
                |(
                    id,
                    provider_id,
                    provider_name,
                    email,
                    plan,
                    auth_method,
                    status,
                    last_refresh,
                )| {
                    Ok(AccountSummary {
                        usage: self.latest_usage(&id)?,
                        id,
                        provider_id,
                        provider_name,
                        email,
                        plan,
                        auth_method,
                        connection_status: parse_connection_status(&status),
                        last_refresh,
                    })
                },
            )
            .collect()
    }

    fn latest_usage(&self, account_id: &str) -> Result<Vec<UsageValue>> {
        let mut statement = self.connection.prepare(
            "SELECT metric, COALESCE(window_type, metric), value, max_value, unit, source, confidence, retrieved_at, reset_at FROM usage_snapshots WHERE account_id = ?1 AND id IN (SELECT MAX(id) FROM usage_snapshots WHERE account_id = ?1 GROUP BY metric) ORDER BY retrieved_at DESC, metric",
        )?;
        let rows = statement.query_map([account_id], usage_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn usage_history(&self, limit: usize) -> Result<Vec<UsageHistoryPoint>> {
        let mut statement = self.connection.prepare(
            "SELECT u.id, u.account_id, a.provider_id, p.display_name, a.display_name, u.metric, COALESCE(u.window_type, u.metric), u.value, u.max_value, u.unit, u.source, u.confidence, u.retrieved_at, u.reset_at FROM usage_snapshots u JOIN accounts a ON a.id = u.account_id JOIN providers p ON p.id = a.provider_id ORDER BY u.retrieved_at DESC, u.id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit as i64], |row| {
            Ok(UsageHistoryPoint {
                id: row.get(0)?,
                account_id: row.get(1)?,
                provider_id: row.get(2)?,
                provider_name: row.get(3)?,
                account_label: row.get(4)?,
                usage: UsageValue {
                    metric: row.get(5)?,
                    label: row.get(6)?,
                    value: row.get(7)?,
                    max_value: row.get(8)?,
                    unit: row.get(9)?,
                    source: parse_usage_source(&row.get::<_, String>(10)?),
                    confidence: parse_confidence(&row.get::<_, String>(11)?),
                    retrieved_at: row.get(12)?,
                    reset_at: row.get(13)?,
                },
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn notifications(&self, limit: usize) -> Result<Vec<ActivityEvent>> {
        let mut statement = self.connection.prepare(
            "SELECT id, event_type, title, detail, severity, created_at FROM activity_events WHERE event_type LIKE 'usage_%' OR event_type = 'provider_error' ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit as i64], activity_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete_account(&mut self, account_id: &str) -> Result<bool> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "UPDATE app_sessions SET account_id = NULL WHERE account_id = ?1",
            [account_id],
        )?;
        tx.execute(
            "UPDATE activity_events SET account_id = NULL WHERE account_id = ?1",
            [account_id],
        )?;
        tx.execute(
            "DELETE FROM notification_rules WHERE account_id = ?1",
            [account_id],
        )?;
        tx.execute(
            "DELETE FROM rate_limit_snapshots WHERE account_id = ?1",
            [account_id],
        )?;
        tx.execute(
            "DELETE FROM usage_snapshots WHERE account_id = ?1",
            [account_id],
        )?;
        let deleted = tx.execute("DELETE FROM accounts WHERE id = ?1", [account_id])? > 0;
        tx.commit()?;
        Ok(deleted)
    }

    pub fn clear_usage_history(&mut self) -> Result<usize> {
        let tx = self.connection.transaction()?;
        let deleted = tx.execute("DELETE FROM usage_snapshots", [])?;
        tx.execute("DELETE FROM rate_limit_snapshots", [])?;
        tx.execute(
            "DELETE FROM activity_events WHERE event_type LIKE 'usage_%'",
            [],
        )?;
        tx.commit()?;
        Ok(deleted)
    }

    pub fn security_status(&self) -> Result<SecurityStatus> {
        let account_count =
            self.connection
                .query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))?;
        let usage_snapshot_count =
            self.connection
                .query_row("SELECT COUNT(*) FROM usage_snapshots", [], |row| row.get(0))?;
        let database_path = self
            .path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "In-memory test database".into());
        let database_size_bytes = self
            .path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        // The Antigravity pool keeps Google OAuth tokens here; report them instead of claiming none.
        let stored_secret_count: i64 = self.connection.query_row(
            "SELECT COALESCE(SUM((access_token <> '') + (refresh_token <> '') + (COALESCE(id_token, '') <> '')), 0) FROM antigravity_accounts",
            [],
            |row| row.get(0),
        )?;
        Ok(SecurityStatus {
            database_path,
            database_size_bytes,
            account_count,
            usage_snapshot_count,
            stored_secret_count,
            provider_credentials_stored: stored_secret_count > 0,
            secrets_encrypted: self.connection.query_row(
                &format!("SELECT COUNT(*) = 0 FROM antigravity_accounts WHERE {PLAIN_TOKEN_FILTER}"),
                [],
                |row| row.get(0),
            )?,
        })
    }

    pub fn record_provider_error(&self, provider: &ProviderSummary) -> Result<()> {
        let settings = self.settings()?;
        let Some(detail) = provider.error.as_deref() else {
            return Ok(());
        };
        if !settings.notifications_enabled || !settings.notify_on_provider_error {
            return Ok(());
        }
        let now = Utc::now().timestamp();
        let title = format!("{} refresh failed", provider.name);
        let duplicate: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM activity_events WHERE event_type = 'provider_error' AND title = ?1 AND detail = ?2 AND created_at >= ?3",
            params![title, detail, now - 3_600],
            |row| row.get(0),
        )?;
        if duplicate == 0 {
            self.connection.execute(
                "INSERT INTO activity_events (event_type, title, detail, severity, created_at) VALUES ('provider_error', ?1, ?2, 'warning', ?3)",
                params![title, detail, now],
            )?;
        }
        Ok(())
    }

    pub fn antigravity_accounts(&self) -> Result<Vec<AntigravityAccount>> {
        let mut stmt = self.connection.prepare(
            "SELECT id, email, display_name, access_token, refresh_token, id_token, project_id, expiry_timestamp, status, is_current, last_used, quota_remaining_percent, quota_details_json FROM antigravity_accounts ORDER BY is_current DESC, last_used DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AntigravityAccount {
                id: row.get(0)?,
                email: row.get(1)?,
                display_name: row.get(2)?,
                access_token: crate::secret::reveal(&row.get::<_, String>(3)?),
                refresh_token: crate::secret::reveal(&row.get::<_, String>(4)?),
                id_token: row.get::<_, Option<String>>(5)?.map(|token| crate::secret::reveal(&token)),
                project_id: row.get(6)?,
                expiry_timestamp: row.get(7)?,
                status: row.get(8)?,
                is_current: row.get::<_, i64>(9)? == 1,
                last_used: row.get(10)?,
                quota_remaining_percent: row.get(11)?,
                quota_details: row.get::<_, Option<String>>(12).ok().flatten().and_then(|s| serde_json::from_str(&s).ok()),
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn antigravity_account(&self, id: &str) -> Result<Option<AntigravityAccount>> {
        let mut stmt = self.connection.prepare(
            "SELECT id, email, display_name, access_token, refresh_token, id_token, project_id, expiry_timestamp, status, is_current, last_used, quota_remaining_percent, quota_details_json FROM antigravity_accounts WHERE id = ?1",
        )?;
        let row = stmt.query_row([id], |row| {
            Ok(AntigravityAccount {
                id: row.get(0)?,
                email: row.get(1)?,
                display_name: row.get(2)?,
                access_token: crate::secret::reveal(&row.get::<_, String>(3)?),
                refresh_token: crate::secret::reveal(&row.get::<_, String>(4)?),
                id_token: row.get::<_, Option<String>>(5)?.map(|token| crate::secret::reveal(&token)),
                project_id: row.get(6)?,
                expiry_timestamp: row.get(7)?,
                status: row.get(8)?,
                is_current: row.get::<_, i64>(9)? == 1,
                last_used: row.get(10)?,
                quota_remaining_percent: row.get(11)?,
                quota_details: row.get::<_, Option<String>>(12).ok().flatten().and_then(|s| serde_json::from_str(&s).ok()),
            })
        }).optional()?;
        Ok(row)
    }

    pub fn save_antigravity_account(&self, account: &AntigravityAccount) -> Result<()> {
        let details_json = account.quota_details.as_ref().and_then(|d| serde_json::to_string(d).ok());
        self.connection.execute(
            r#"
            INSERT INTO antigravity_accounts (
                id, email, display_name, access_token, refresh_token, id_token,
                project_id, expiry_timestamp, status, is_current, last_used, quota_remaining_percent,
                quota_details_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(id) DO UPDATE SET
                email = excluded.email,
                display_name = COALESCE(excluded.display_name, antigravity_accounts.display_name),
                access_token = excluded.access_token,
                refresh_token = excluded.refresh_token,
                id_token = COALESCE(excluded.id_token, antigravity_accounts.id_token),
                project_id = COALESCE(excluded.project_id, antigravity_accounts.project_id),
                expiry_timestamp = excluded.expiry_timestamp,
                status = excluded.status,
                is_current = excluded.is_current,
                last_used = excluded.last_used,
                quota_remaining_percent = COALESCE(excluded.quota_remaining_percent, antigravity_accounts.quota_remaining_percent),
                quota_details_json = COALESCE(excluded.quota_details_json, antigravity_accounts.quota_details_json)
            "#,
            params![
                account.id,
                account.email,
                account.display_name,
                crate::secret::protect(&account.access_token),
                crate::secret::protect(&account.refresh_token),
                account.id_token.as_deref().map(crate::secret::protect),
                account.project_id,
                account.expiry_timestamp,
                account.status,
                if account.is_current { 1 } else { 0 },
                account.last_used,
                account.quota_remaining_percent,
                details_json,
            ],
        )?;
        Ok(())
    }

    pub fn set_current_antigravity_account(&self, id: &str) -> Result<()> {
        self.connection.execute("UPDATE antigravity_accounts SET is_current = 0", [])?;
        self.connection.execute(
            "UPDATE antigravity_accounts SET is_current = 1, last_used = ?2 WHERE id = ?1",
            params![id, Utc::now().timestamp()],
        )?;
        Ok(())
    }

    pub fn delete_antigravity_account(&self, id: &str) -> Result<bool> {
        let count = self
            .connection
            .execute("DELETE FROM antigravity_accounts WHERE id = ?1", [id])?;
        Ok(count > 0)
    }

    pub fn record_activity_event(
        &self,
        event_type: &str,
        account_id: Option<&str>,
        title: &str,
        detail: Option<&str>,
        severity: Severity,
        now: i64,
    ) -> Result<()> {
        let severity_str = match severity {
            Severity::Success => "success",
            Severity::Warning => "warning",
            Severity::Danger => "danger",
            Severity::Info => "info",
        };
        self.connection.execute(
            "INSERT INTO activity_events (event_type, account_id, title, detail, severity, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![event_type, account_id, title, detail, severity_str, now],
        )?;
        Ok(())
    }
}

fn usage_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UsageValue> {
    Ok(UsageValue {
        metric: row.get(0)?,
        label: row.get(1)?,
        value: row.get(2)?,
        max_value: row.get(3)?,
        unit: row.get(4)?,
        source: parse_usage_source(&row.get::<_, String>(5)?),
        confidence: parse_confidence(&row.get::<_, String>(6)?),
        retrieved_at: row.get(7)?,
        reset_at: row.get(8)?,
    })
}

fn activity_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ActivityEvent> {
    let severity: String = row.get(4)?;
    Ok(ActivityEvent {
        id: row.get(0)?,
        event_type: row.get(1)?,
        title: row.get(2)?,
        detail: row.get(3)?,
        severity: match severity.as_str() {
            "success" => Severity::Success,
            "warning" => Severity::Warning,
            "danger" => Severity::Danger,
            _ => Severity::Info,
        },
        created_at: row.get(5)?,
    })
}

fn record_usage_alert(
    tx: &Transaction<'_>,
    account_id: &str,
    provider: &ProviderSummary,
    usage: &UsageValue,
    previous: Option<f64>,
    settings: &AppSettings,
) -> Result<()> {
    if !settings.notifications_enabled || usage.max_value != Some(100.0) {
        return Ok(());
    }
    let Some(value) = usage.value else {
        return Ok(());
    };
    let warning = f64::from(settings.notify_warning_percent);
    let critical = f64::from(settings.notify_critical_percent);
    let (event_type, severity) = if value <= critical && previous.is_none_or(|old| old > critical) {
        (Some("usage_critical"), "danger")
    } else if value <= warning && previous.is_none_or(|old| old > warning) {
        (Some("usage_warning"), "warning")
    } else if settings.notify_on_reset && previous.is_some_and(|old| value - old >= 25.0) {
        (Some("usage_reset"), "success")
    } else {
        (None, "info")
    };
    if let Some(event_type) = event_type {
        let action = if event_type == "usage_reset" {
            format!("{} reset to {}% remaining", usage.label, value.round())
        } else {
            format!("{} has {}% remaining", usage.label, value.round())
        };
        tx.execute(
            "INSERT INTO activity_events (event_type, account_id, title, detail, severity, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![event_type, account_id, format!("{} usage alert", provider.name), action, severity, usage.retrieved_at],
        )?;
    }
    Ok(())
}

fn parse_connection_status(value: &str) -> ConnectionStatus {
    match value {
        "connected" => ConnectionStatus::Connected,
        "error" => ConnectionStatus::Error,
        _ => ConnectionStatus::NotConnected,
    }
}

fn parse_usage_source(value: &str) -> UsageSource {
    match value {
        "official_api" => UsageSource::OfficialApi,
        "official_cli" => UsageSource::OfficialCli,
        "local_application" => UsageSource::LocalApplication,
        "local_logs" => UsageSource::LocalLogs,
        "locally_calculated" => UsageSource::LocallyCalculated,
        "estimated" => UsageSource::Estimated,
        _ => UsageSource::Unavailable,
    }
}

fn parse_confidence(value: &str) -> Confidence {
    match value {
        "high" => Confidence::High,
        "medium" => Confidence::Medium,
        "low" => Confidence::Low,
        _ => Confidence::Unavailable,
    }
}

fn connection_status_name(value: &ConnectionStatus) -> &'static str {
    match value {
        ConnectionStatus::Connected => "connected",
        ConnectionStatus::NotConnected => "not_connected",
        ConnectionStatus::Error => "error",
    }
}

fn usage_source_name(value: &UsageSource) -> &'static str {
    match value {
        UsageSource::OfficialApi => "official_api",
        UsageSource::OfficialCli => "official_cli",
        UsageSource::LocalApplication => "local_application",
        UsageSource::LocalLogs => "local_logs",
        UsageSource::LocallyCalculated => "locally_calculated",
        UsageSource::Estimated => "estimated",
        UsageSource::Unavailable => "unavailable",
    }
}

fn confidence_name(value: &Confidence) -> &'static str {
    match value {
        Confidence::High => "high",
        Confidence::Medium => "medium",
        Confidence::Low => "low",
        Confidence::Unavailable => "unavailable",
    }
}

fn insert_activity(
    tx: &Transaction<'_>,
    event_type: &str,
    app_id: &str,
    title: &str,
    detail: Option<&str>,
    severity: &str,
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO activity_events (event_type, app_id, title, detail, severity, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![event_type, app_id, title, detail, severity, now],
    )?;
    Ok(())
}

fn enrich_status(tx: &Transaction<'_>, status: &mut AppStatus, now: i64) -> Result<()> {
    let local_now = Local
        .timestamp_opt(now, 0)
        .single()
        .unwrap_or_else(Local::now);
    let today = local_now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid");
    let today_start = Local
        .from_local_datetime(&today)
        .earliest()
        .unwrap_or(local_now)
        .timestamp();
    let week_start = today_start
        - Duration::days(local_now.weekday().num_days_from_monday().into()).num_seconds();

    status.today_seconds = duration_since(tx, &status.id, today_start, now)?;
    status.week_seconds = duration_since(tx, &status.id, week_start, now)?;
    status.last_used_at = tx.query_row(
        "SELECT MAX(COALESCE(ended_at, started_at)) FROM app_sessions WHERE app_id = ?1",
        [&status.id],
        |row| row.get(0),
    )?;
    status.current_session_seconds = status
        .running_since
        .map(|start| (now - start).max(0))
        .unwrap_or(0);
    Ok(())
}

fn duration_since(tx: &Transaction<'_>, app_id: &str, since: i64, now: i64) -> Result<i64> {
    Ok(tx.query_row(
        "SELECT COALESCE(SUM(MAX(0, MIN(COALESCE(ended_at, ?3), ?3) - MAX(started_at, ?2))), 0) FROM app_sessions WHERE app_id = ?1 AND COALESCE(ended_at, ?3) >= ?2",
        params![app_id, since, now],
        |row| row.get(0),
    )?)
}

fn seed_applications() -> Vec<ApplicationDefinition> {
    [
        ("codex", Some("openai"), "Codex", &["codex.exe"][..]),
        ("chatgpt", Some("openai"), "ChatGPT", &["chatgpt.exe"][..]),
        (
            "claude",
            Some("anthropic"),
            "Claude Desktop",
            &["claude.exe"][..],
        ),
        (
            "claude-code",
            Some("anthropic"),
            "Claude Code",
            &["claude-code.exe"][..],
        ),
        (
            "gemini",
            Some("google"),
            "Gemini CLI",
            &["gemini.exe", "gemini.cmd"][..],
        ),
        (
            "antigravity",
            Some("google"),
            "Antigravity",
            &["antigravity.exe", "antigravity ide.exe"][..],
        ),
        (
            "opencode",
            Some("opencode"),
            "OpenCode",
            &["opencode.exe"][..],
        ),
        ("cursor", None, "Cursor", &["cursor.exe"][..]),
        (
            "vscode",
            Some("github"),
            "Visual Studio Code",
            &["code.exe"][..],
        ),
        ("kimi", Some("kimi"), "Kimi", &["kimi.exe"][..]),
    ]
    .into_iter()
    .map(
        |(id, provider_id, display_name, names)| ApplicationDefinition {
            id: id.into(),
            provider_id: provider_id.map(str::to_string),
            display_name: display_name.into(),
            executable_path: None,
            launch_arguments: vec![],
            process_names: names.iter().map(|name| (*name).into()).collect(),
        },
    )
    .collect()
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn pool_tokens_are_encrypted_at_rest_and_legacy_rows_are_upgraded() {
        let database = Database::memory().unwrap();
        let account = AntigravityAccount {
            id: "antigravity:a@example.com".into(),
            email: "a@example.com".into(),
            display_name: None,
            access_token: "access-secret".into(),
            refresh_token: "refresh-secret".into(),
            id_token: Some("id-secret".into()),
            project_id: None,
            expiry_timestamp: 0,
            status: "healthy".into(),
            is_current: false,
            last_used: 0,
            quota_remaining_percent: None,
            quota_details: None,
        };
        database.save_antigravity_account(&account).unwrap();
        let raw: String = database
            .connection
            .query_row("SELECT access_token || refresh_token || id_token FROM antigravity_accounts", [], |row| row.get(0))
            .unwrap();
        assert!(!raw.contains("secret"), "tokens must not be stored in plain text");
        let loaded = database.antigravity_account(&account.id).unwrap().unwrap();
        assert_eq!((loaded.access_token.as_str(), loaded.refresh_token.as_str(), loaded.id_token.as_deref()), ("access-secret", "refresh-secret", Some("id-secret")));
        assert!(database.security_status().unwrap().secrets_encrypted);

        // A row written by an older version in plain text is upgraded by the migration.
        database.connection.execute("UPDATE antigravity_accounts SET refresh_token = 'old-plain'", []).unwrap();
        assert!(!database.security_status().unwrap().secrets_encrypted);
        database.encrypt_legacy_tokens().unwrap();
        assert!(database.security_status().unwrap().secrets_encrypted);
        assert_eq!(database.antigravity_account(&account.id).unwrap().unwrap().refresh_token, "old-plain");
    }

    use super::*;
    use crate::models::{ProviderCapabilities, UsageValue};

    fn status(state: RunningState) -> AppStatus {
        AppStatus {
            id: "codex".into(),
            provider_id: Some("openai".into()),
            display_name: "Codex".into(),
            state,
            detected_executable: Some("Codex.exe".into()),
            executable_path: None,
            running_since: None,
            current_session_seconds: 0,
            today_seconds: 0,
            week_seconds: 0,
            last_used_at: None,
            launchable: false,
        }
    }

    fn provider(
        account_key: &str,
        email: &str,
        remaining: f64,
        retrieved_at: i64,
    ) -> ProviderSummary {
        ProviderSummary {
            id: "openai".into(),
            name: "OpenAI".into(),
            account_key: Some(account_key.into()),
            account_label: email.into(),
            plan: Some("Plus".into()),
            connection_status: ConnectionStatus::Connected,
            capabilities: ProviderCapabilities::default(),
            usage: vec![UsageValue {
                metric: "codex_300m_remaining".into(),
                label: "5-hour window".into(),
                value: Some(remaining),
                max_value: Some(100.0),
                unit: "% remaining".into(),
                source: UsageSource::OfficialCli,
                confidence: Confidence::High,
                retrieved_at,
                reset_at: None,
            }],
            usage_allowed: Some(true),
            last_refresh: Some(retrieved_at),
            error: None,
        }
    }

    #[test]
    fn english_default_keeps_saved_language_on_upgrade() -> Result<()> {
        let mut db = Database::memory()?;
        assert_eq!(db.settings()?.language, "en");
        let turkish = AppSettings { language: "tr".into(), ..AppSettings::default() };
        db.save_settings(&turkish)?;
        db.migrate()?;
        assert_eq!(db.settings()?.language, "tr");
        let english = AppSettings::default();
        db.save_settings(&english)?;
        db.migrate()?;
        assert_eq!(db.settings()?.language, "en");
        Ok(())
    }

    #[test]
    fn process_transitions_create_and_close_one_session() -> Result<()> {
        let mut db = Database::memory()?;
        let mut running = vec![status(RunningState::Running)];
        assert!(db.sync_sessions(&mut running, 1_700_000_000)?);
        assert!(!db.sync_sessions(&mut running, 1_700_000_030)?);
        let mut stopped = vec![status(RunningState::NotRunning)];
        assert!(db.sync_sessions(&mut stopped, 1_700_000_090)?);
        let duration: i64 = db.connection.query_row(
            "SELECT duration_seconds FROM app_sessions WHERE app_id = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(duration, 90);
        assert_eq!(db.activity(10)?.len(), 2);
        Ok(())
    }

    #[test]
    fn account_switches_preserve_each_latest_state() -> Result<()> {
        let mut db = Database::memory()?;
        db.save_provider_snapshot(&provider("account-a", "a@example.com", 70.0, 100))?;
        db.save_provider_snapshot(&provider("account-b", "b@example.com", 30.0, 200))?;
        db.save_provider_snapshot(&provider("account-a", "a@example.com", 60.0, 300))?;

        let account_count: i64 =
            db.connection
                .query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))?;
        let active_email: String = db.connection.query_row(
            "SELECT display_name FROM accounts WHERE connection_status = 'connected'",
            [],
            |row| row.get(0),
        )?;
        let latest_a: f64 = db.connection.query_row(
            "SELECT value FROM usage_snapshots WHERE account_id = 'openai:account-a' ORDER BY retrieved_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        let latest_b: f64 = db.connection.query_row(
            "SELECT value FROM usage_snapshots WHERE account_id = 'openai:account-b' ORDER BY retrieved_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(account_count, 2);
        assert_eq!(active_email, "a@example.com");
        assert_eq!(latest_a, 60.0);
        assert_eq!(latest_b, 30.0);
        let accounts = db.accounts()?;
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].email, "a@example.com");
        assert_eq!(accounts[0].usage[0].value, Some(60.0));

        let mut signed_out = provider("unused", "unused@example.com", 0.0, 400);
        signed_out.account_key = None;
        signed_out.connection_status = ConnectionStatus::NotConnected;
        signed_out.usage.clear();
        db.save_provider_snapshot(&signed_out)?;
        let active_count: i64 = db.connection.query_row(
            "SELECT COUNT(*) FROM accounts WHERE connection_status = 'connected'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(active_count, 0);
        assert!(db.delete_account("openai:account-b")?);
        assert_eq!(db.accounts()?.len(), 1);
        Ok(())
    }

    #[test]
    fn device_log_totals_are_not_attributed_to_an_account() -> Result<()> {
        let mut db = Database::memory()?;
        let mut snapshot = provider("account-a", "a@example.com", 70.0, 100);
        snapshot.usage.push(UsageValue {
            metric: "codex_recent_tokens".into(),
            label: "Device tokens · all accounts".into(),
            value: Some(1_000.0),
            max_value: None,
            unit: "tokens".into(),
            source: UsageSource::LocalLogs,
            confidence: Confidence::High,
            retrieved_at: 100,
            reset_at: None,
        });
        db.save_provider_snapshot(&snapshot)?;
        assert_eq!(db.accounts()?[0].usage.len(), 1);
        Ok(())
    }
}
