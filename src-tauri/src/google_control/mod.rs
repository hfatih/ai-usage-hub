mod models;
#[cfg(test)]
mod tests;
mod transfer;
pub use models::*;
pub use transfer::{export_metadata, import_metadata};

use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

pub type Result<T> = std::result::Result<T, String>;
pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn now() -> i64 {
    Utc::now().timestamp()
}
fn id(prefix: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{prefix}-{}-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

pub const SERVICES: &[(&str, &str, &str)] = &[
    ("gemini", "Gemini", "https://gemini.google.com/"),
    ("flow", "Flow", "https://labs.google/fx/tools/flow"),
    ("notebooklm", "NotebookLM", "https://notebooklm.google.com/"),
    (
        "aistudio",
        "Google AI Studio",
        "https://aistudio.google.com/",
    ),
    ("antigravity", "Antigravity", "https://antigravity.google/"),
    (
        "gemini_cli",
        "Gemini CLI",
        "https://github.com/google-gemini/gemini-cli",
    ),
    ("drive", "Google Drive", "https://drive.google.com/"),
    ("googleone", "Google One", "https://one.google.com/"),
];

pub fn service_url(service_id: &str) -> Result<&'static str> {
    SERVICES
        .iter()
        .find(|s| s.0 == service_id)
        .map(|s| s.2)
        .ok_or_else(|| "Unknown Google service".into())
}

pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(r#"
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS google_accounts (
            id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE COLLATE NOCASE,
            metadata_json TEXT NOT NULL, antigravity_id TEXT, last_used INTEGER,
            created_at INTEGER NOT NULL, quota_remaining REAL, quota_updated_at INTEGER, quota_details TEXT
        );
        CREATE TABLE IF NOT EXISTS google_projects (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL,
            notes TEXT NOT NULL, status TEXT NOT NULL,
            created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS google_assignments (
            project_id TEXT NOT NULL REFERENCES google_projects(id) ON DELETE CASCADE,
            account_id TEXT NOT NULL REFERENCES google_accounts(id) ON DELETE CASCADE,
            role TEXT NOT NULL, services_json TEXT NOT NULL, PRIMARY KEY(project_id, account_id)
        );
        CREATE TABLE IF NOT EXISTS google_usage (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id TEXT NOT NULL REFERENCES google_accounts(id) ON DELETE CASCADE,
            service_id TEXT NOT NULL, metric TEXT NOT NULL, value REAL NOT NULL,
            max_value REAL, unit TEXT NOT NULL, recorded_at INTEGER NOT NULL,
            source TEXT NOT NULL CHECK(source IN ('manual','antigravity'))
        );
        CREATE INDEX IF NOT EXISTS google_usage_time ON google_usage(recorded_at DESC);
        CREATE TABLE IF NOT EXISTS google_service_preferences (
            service_id TEXT PRIMARY KEY, favorite INTEGER NOT NULL DEFAULT 0,
            last_account_id TEXT REFERENCES google_accounts(id) ON DELETE SET NULL
        );
        CREATE TABLE IF NOT EXISTS google_service_status (
            service_id TEXT NOT NULL, account_id TEXT NOT NULL REFERENCES google_accounts(id) ON DELETE CASCADE,
            status TEXT NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY(service_id,account_id)
        );
        CREATE TABLE IF NOT EXISTS google_activity (
            id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL,
            account_id TEXT REFERENCES google_accounts(id) ON DELETE SET NULL,
            project_id TEXT REFERENCES google_projects(id) ON DELETE SET NULL,
            title TEXT NOT NULL, detail TEXT NOT NULL, created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS google_preferences (id INTEGER PRIMARY KEY CHECK(id=1), json TEXT NOT NULL);
    "#).map_err(err)?;
    for service in SERVICES {
        conn.execute(
            "INSERT OR IGNORE INTO google_service_preferences(service_id) VALUES(?1)",
            [service.0],
        )
        .map_err(err)?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO google_preferences(id,json) VALUES(1,?1)",
        [serde_json::to_string(&GooglePreferences::default()).map_err(err)?],
    )
    .map_err(err)?;
    Ok(())
}

fn text(value: &str, max: usize, name: &str) -> Result<()> {
    if value.len() > max || value.contains('\0') {
        return Err(format!("Invalid {name}"));
    }
    Ok(())
}

pub(crate) fn validate_account(a: &mut GoogleAccountInput) -> Result<()> {
    a.email = a.email.trim().to_lowercase();
    let email = a.email.as_bytes();
    let parts: Vec<_> = a.email.split('@').collect();
    if email.len() > 254
        || parts.len() != 2
        || parts[0].is_empty()
        || parts[1].is_empty()
        || !parts[1].contains('.')
        || parts[1].starts_with('.')
        || parts[1].ends_with('.')
        || email
            .iter()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return Err("A valid email address is required".into());
    }
    if parts[0].len() > 64
        || parts[0].starts_with('.')
        || parts[0].ends_with('.')
        || parts[0].contains("..")
        || !parts[0]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&b))
        || !parts[1].split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return Err("A valid email address is required".into());
    }
    text(&a.name, 200, "name")?;
    text(&a.subscription, 200, "subscription")?;
    text(&a.group, 200, "group")?;
    text(&a.notes, 100_000, "notes")?;
    if a.color.len() != 7
        || !a.color.starts_with('#')
        || !a.color[1..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Color must be #RRGGBB".into());
    }
    for date in [&a.activation_date, &a.expiration_date]
        .into_iter()
        .flatten()
    {
        if date.len() != 10 || NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
            return Err("Dates must be YYYY-MM-DD".into());
        }
    }
    if let (Some(start), Some(end)) = (&a.activation_date, &a.expiration_date) {
        if end < start {
            return Err("Expiration cannot precede activation".into());
        }
    }
    if a.tags.len() > 50 {
        return Err("At most 50 tags are allowed".into());
    }
    for tag in &mut a.tags {
        *tag = tag.trim().to_string();
        text(tag, 100, "tag")?;
        if tag.is_empty() {
            return Err("Tags cannot be empty".into());
        }
    }
    let mut seen = HashSet::new();
    a.tags.retain(|tag| seen.insert(tag.to_lowercase()));
    if let Some(value) = &a.id {
        text(value, 200, "account id")?;
        if value.is_empty() {
            return Err("Account id cannot be empty".into());
        }
    }
    Ok(())
}

fn account_exists(conn: &Connection, account_id: &str) -> Result<()> {
    if !conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM google_accounts WHERE id=?1)",
            [account_id],
            |r| r.get::<_, bool>(0),
        )
        .map_err(err)?
    {
        return Err("Google account does not exist".into());
    }
    Ok(())
}

pub(crate) fn activity(
    conn: &Connection,
    kind: &str,
    account: Option<&str>,
    project: Option<&str>,
    title: &str,
    detail: &str,
) -> Result<()> {
    conn.execute("INSERT INTO google_activity(kind,account_id,project_id,title,detail,created_at) VALUES(?1,?2,?3,?4,?5,?6)", params![kind,account,project,title,detail,now()]).map_err(err)?;
    Ok(())
}

pub(crate) fn upsert_account(
    conn: &Connection,
    mut account: GoogleAccountInput,
    allow_new_id: bool,
) -> Result<String> {
    validate_account(&mut account)?;
    if let Some(account_id) = &account.id {
        let original_email: Option<String> = conn
            .query_row(
                "SELECT email FROM google_accounts WHERE id=?1",
                [account_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        if original_email.is_some_and(|email| email.trim().to_lowercase() != account.email) {
            return Err("Add a separate account to use a different email.".into());
        }
    }
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM google_accounts WHERE email=?1 COLLATE NOCASE",
            [&account.email],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?;
    let account_id = match (&account.id, existing) {
        (Some(explicit), Some(found)) if explicit != &found => {
            return Err("Email is already assigned to another account".into())
        }
        (_, Some(found)) => found,
        (Some(explicit), None) => {
            if !allow_new_id {
                account_exists(conn, explicit)?;
            }
            explicit.clone()
        }
        (None, None) => id("google"),
    };
    account.id = Some(account_id.clone());
    conn.execute("INSERT INTO google_accounts(id,email,metadata_json,created_at) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET email=excluded.email,metadata_json=excluded.metadata_json",
        params![account_id,account.email,serde_json::to_string(&account).map_err(err)?,now()]).map_err(err)?;
    Ok(account_id)
}

pub fn save_account(conn: &Connection, account: GoogleAccountInput) -> Result<()> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    let account_id = upsert_account(&tx, account, false)?;
    activity(
        &tx,
        "account_saved",
        Some(&account_id),
        None,
        "Account saved",
        "",
    )?;
    tx.commit().map_err(err)
}

pub fn remove_account(conn: &Connection, account_id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    let data: String = tx
        .query_row(
            "SELECT metadata_json FROM google_accounts WHERE id=?1",
            [account_id],
            |r| r.get(0),
        )
        .map_err(err)?;
    let mut account: GoogleAccountInput = serde_json::from_str(&data).map_err(err)?;
    account.archived = true;
    account.enabled = false;
    upsert_account(&tx, account, false)?;
    activity(
        &tx,
        "account_archived",
        Some(account_id),
        None,
        "Account archived",
        "",
    )?;
    tx.commit().map_err(err)
}

pub fn sync(conn: &Connection) -> Result<()> {
    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='antigravity_accounts')",[],|r|r.get(0)).map_err(err)?;
    if !exists {
        return Ok(());
    }
    // Only quota and identity metadata are selected; authentication material never enters this module.
    let mut stmt = conn.prepare("SELECT id,email,display_name,tier,last_used,quota_remaining_percent,quota_details_json FROM antigravity_accounts ORDER BY last_used DESC,id ASC").map_err(err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<f64>>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    drop(stmt);
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute("UPDATE google_accounts SET antigravity_id=NULL,quota_remaining=NULL,quota_updated_at=NULL,quota_details=NULL WHERE antigravity_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM antigravity_accounts a WHERE a.id=google_accounts.antigravity_id AND lower(trim(a.email))=google_accounts.email)",[]).map_err(err)?;
    let mut seen_emails = HashSet::new();
    for (native_id, email, name, tier, last_used, quota, details) in rows {
        let normalized_email = email.trim().to_lowercase();
        if !seen_emails.insert(normalized_email.clone()) {
            continue;
        }
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM google_accounts WHERE email=?1 COLLATE NOCASE",
                [&normalized_email],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        let account_id = if let Some(found) = existing {
            found
        } else {
            let mut input = GoogleAccountInput {
                id: None,
                email: normalized_email,
                name: name.unwrap_or_default(),
                color: "#8b5cf6".into(),
                subscription: tier.unwrap_or_default(),
                activation_date: None,
                expiration_date: None,
                enabled: true,
                archived: false,
                favorite: false,
                group: String::new(),
                tags: vec![],
                notes: String::new(),
            };
            if validate_account(&mut input).is_err() {
                continue;
            }
            let new_id = upsert_account(&tx, input, false)?;
            activity(
                &tx,
                "account_synced",
                Some(&new_id),
                None,
                "Antigravity account linked",
                "",
            )?;
            new_id
        };
        let quota = quota.filter(|v| v.is_finite() && *v >= 0.0 && *v <= 100.0);
        let previous: Option<f64> = tx
            .query_row(
                "SELECT quota_remaining FROM google_accounts WHERE id=?1",
                [&account_id],
                |r| r.get(0),
            )
            .map_err(err)?;
        let details = details.and_then(|s| sanitize_quota(&s));
        // This timestamp records observation of the local native cache, not a provider refresh.
        tx.execute("UPDATE google_accounts SET antigravity_id=?1,last_used=MAX(COALESCE(last_used,0),?2),quota_remaining=?3,quota_details=?4,quota_updated_at=CASE WHEN quota_remaining IS NOT ?3 OR quota_updated_at IS NULL THEN ?5 ELSE quota_updated_at END WHERE id=?6",params![native_id,last_used,quota,details,now(),account_id]).map_err(err)?;
        if quota != previous {
            if let Some(value) = quota {
                tx.execute("INSERT INTO google_usage(account_id,service_id,metric,value,max_value,unit,recorded_at,source) VALUES(?1,'antigravity','quota_remaining',?2,100,'%',?3,'antigravity')",params![account_id,value,now()]).map_err(err)?;
            }
        }
    }
    tx.commit().map_err(err)
}

/// Applies Google enabled/archive preferences without reading or modifying native credentials.
/// An unlinked native identity remains eligible by default.
pub fn native_account_enabled(conn: &Connection, native_id: &str) -> Result<bool> {
    let mut statement = conn.prepare(
        "SELECT metadata_json FROM google_accounts WHERE antigravity_id=?1 OR email=(SELECT lower(trim(email)) FROM antigravity_accounts WHERE id=?1) COLLATE NOCASE"
    ).map_err(err)?;
    let rows = statement
        .query_map([native_id], |row| row.get::<_, String>(0))
        .map_err(err)?;
    for row in rows {
        let account: GoogleAccountInput = serde_json::from_str(&row.map_err(err)?).map_err(err)?;
        if !account.enabled || account.archived {
            return Ok(false);
        }
    }
    Ok(true)
}

fn sanitize_quota(raw: &str) -> Option<String> {
    fn clean(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.retain(|k, _| {
                    let k = k.to_ascii_lowercase();
                    ![
                        "token",
                        "password",
                        "cookie",
                        "secret",
                        "authorization",
                        "credential",
                    ]
                    .iter()
                    .any(|needle| k.contains(needle))
                });
                for v in map.values_mut() {
                    clean(v);
                }
            }
            serde_json::Value::Array(values) => {
                for v in values {
                    clean(v);
                }
            }
            _ => (),
        }
    }
    let mut value: serde_json::Value = serde_json::from_str(raw).ok()?;
    clean(&mut value);
    serde_json::to_string(&value).ok()
}

pub fn save_project(conn: &Connection, project: GoogleProjectInput) -> Result<()> {
    if project.name.trim().is_empty()
        || !["active", "paused", "archived"].contains(&project.status.as_str())
    {
        return Err("Project needs a name and valid status".into());
    }
    text(&project.name, 200, "project name")?;
    text(&project.description, 10_000, "description")?;
    text(&project.notes, 100_000, "notes")?;
    let project_id = project.id.clone().unwrap_or_else(|| id("project"));
    text(&project_id, 200, "project id")?;
    if project_id.is_empty() || project.assignments.len() > 10_000 {
        return Err("Invalid project".into());
    }
    let mut seen = HashSet::new();
    for assignment in &project.assignments {
        account_exists(conn, &assignment.account_id)?;
        if !seen.insert(&assignment.account_id) {
            return Err("Account can only be assigned once per project".into());
        }
        text(&assignment.role, 200, "role")?;
        if assignment.services.len() > SERVICES.len() {
            return Err("Too many services".into());
        }
        let mut services = HashSet::new();
        for service in &assignment.services {
            service_url(service)?;
            if !services.insert(service) {
                return Err("Duplicate assigned service".into());
            }
        }
    }
    let tx = conn.unchecked_transaction().map_err(err)?;
    if project.id.is_some() {
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM google_projects WHERE id=?1)",
                [&project_id],
                |r| r.get(0),
            )
            .map_err(err)?;
        if !exists {
            return Err("Project does not exist".into());
        }
    }
    tx.execute("INSERT INTO google_projects(id,name,description,notes,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6) ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,notes=excluded.notes,status=excluded.status,updated_at=excluded.updated_at",params![project_id,project.name.trim(),project.description,project.notes,project.status,now()]).map_err(err)?;
    tx.execute(
        "DELETE FROM google_assignments WHERE project_id=?1",
        [&project_id],
    )
    .map_err(err)?;
    for a in project.assignments {
        tx.execute("INSERT INTO google_assignments(project_id,account_id,role,services_json) VALUES(?1,?2,?3,?4)",params![project_id,a.account_id,a.role,serde_json::to_string(&a.services).map_err(err)?]).map_err(err)?;
    }
    activity(
        &tx,
        "project_saved",
        None,
        Some(&project_id),
        "Project saved",
        "",
    )?;
    tx.commit().map_err(err)
}

pub fn delete_project(conn: &Connection, project_id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    if tx
        .execute("DELETE FROM google_projects WHERE id=?1", [project_id])
        .map_err(err)?
        == 0
    {
        return Err("Project does not exist".into());
    }
    activity(
        &tx,
        "project_deleted",
        None,
        None,
        "Project deleted",
        project_id,
    )?;
    tx.commit().map_err(err)
}

pub fn add_usage(conn: &Connection, record: GoogleUsageInput) -> Result<()> {
    account_exists(conn, &record.account_id)?;
    service_url(&record.service_id)?;
    text(&record.metric, 200, "metric")?;
    text(&record.unit, 100, "unit")?;
    if record.metric.trim().is_empty()
        || !record.value.is_finite()
        || record.value < 0.0
        || record
            .max_value
            .is_some_and(|v| !v.is_finite() || v <= 0.0 || v < record.value)
    {
        return Err("Usage requires finite nonnegative values and a valid maximum".into());
    }
    let recorded_at = record.recorded_at.unwrap_or_else(now);
    if recorded_at < 0 || recorded_at > now() + 300 {
        return Err("Invalid usage timestamp".into());
    }
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute("INSERT INTO google_usage(account_id,service_id,metric,value,max_value,unit,recorded_at,source) VALUES(?1,?2,?3,?4,?5,?6,?7,'manual')",params![record.account_id,record.service_id,record.metric.trim(),record.value,record.max_value,record.unit,recorded_at]).map_err(err)?;
    activity(
        &tx,
        "usage_added",
        Some(&record.account_id),
        None,
        "Manual usage recorded",
        &record.metric,
    )?;
    tx.commit().map_err(err)
}

pub fn delete_usage(conn: &Connection, record_id: i64) -> Result<()> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    let (account, source): (String, String) = tx
        .query_row(
            "SELECT account_id,source FROM google_usage WHERE id=?1",
            [record_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(err)?;
    if source != "manual" {
        return Err("Native quota history cannot be deleted manually".into());
    }
    tx.execute("DELETE FROM google_usage WHERE id=?1", [record_id])
        .map_err(err)?;
    activity(
        &tx,
        "usage_deleted",
        Some(&account),
        None,
        "Manual usage deleted",
        "",
    )?;
    tx.commit().map_err(err)
}

pub fn check_launch(conn: &Connection, account_id: &str, service_id: &str) -> Result<()> {
    service_url(service_id)?;
    let json: String = conn
        .query_row(
            "SELECT metadata_json FROM google_accounts WHERE id=?1",
            [account_id],
            |r| r.get(0),
        )
        .map_err(err)?;
    let a: GoogleAccountInput = serde_json::from_str(&json).map_err(err)?;
    if !a.enabled || a.archived {
        return Err("Enable and unarchive this account before launching a service".into());
    }
    Ok(())
}

pub fn record_launch(conn: &Connection, account_id: &str, service_id: &str) -> Result<()> {
    check_launch(conn, account_id, service_id)?;
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute(
        "UPDATE google_accounts SET last_used=?1 WHERE id=?2",
        params![now(), account_id],
    )
    .map_err(err)?;
    tx.execute(
        "UPDATE google_service_preferences SET last_account_id=?1 WHERE service_id=?2",
        params![account_id, service_id],
    )
    .map_err(err)?;
    activity(
        &tx,
        "service_launched",
        Some(account_id),
        None,
        "Service opened",
        service_id,
    )?;
    tx.commit().map_err(err)
}

pub fn set_service_status(
    conn: &Connection,
    account_id: &str,
    service_id: &str,
    status: &str,
) -> Result<()> {
    account_exists(conn, account_id)?;
    service_url(service_id)?;
    if !["unknown", "available", "login_required", "unavailable"].contains(&status) {
        return Err("Invalid service status".into());
    }
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute("INSERT INTO google_service_status(service_id,account_id,status,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(service_id,account_id) DO UPDATE SET status=excluded.status,updated_at=excluded.updated_at",params![service_id,account_id,status,now()]).map_err(err)?;
    activity(
        &tx,
        "service_status",
        Some(account_id),
        None,
        "Manual service status updated",
        &format!("{service_id}: {status}"),
    )?;
    tx.commit().map_err(err)
}

pub fn favorite_service(conn: &Connection, service_id: &str, favorite: bool) -> Result<()> {
    service_url(service_id)?;
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute(
        "UPDATE google_service_preferences SET favorite=?1 WHERE service_id=?2",
        params![favorite, service_id],
    )
    .map_err(err)?;
    activity(
        &tx,
        "service_favorite",
        None,
        None,
        "Service favorite updated",
        service_id,
    )?;
    tx.commit().map_err(err)
}

pub fn preferences(conn: &Connection) -> Result<GooglePreferences> {
    let json: String = conn
        .query_row("SELECT json FROM google_preferences WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(err)?;
    serde_json::from_str(&json).map_err(err)
}

pub fn save_preferences(conn: &Connection, preferences: GooglePreferences) -> Result<()> {
    if !(0..=365).contains(&preferences.expiration_warning_days)
        || !(1..=100).contains(&preferences.backup_retention)
    {
        return Err("Warning days must be 0–365 and backup retention 1–100".into());
    }
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute(
        "UPDATE google_preferences SET json=?1 WHERE id=1",
        [serde_json::to_string(&preferences).map_err(err)?],
    )
    .map_err(err)?;
    activity(
        &tx,
        "preferences_saved",
        None,
        None,
        "Preferences saved",
        "",
    )?;
    tx.commit().map_err(err)
}

pub fn snapshot(conn: &Connection) -> Result<GoogleSnapshot> {
    sync(conn)?;
    let mut stmt=conn.prepare("SELECT metadata_json,antigravity_id,last_used,created_at,quota_remaining,quota_updated_at,quota_details FROM google_accounts ORDER BY created_at,id").map_err(err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<f64>>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(err)?;
    let mut accounts = vec![];
    for row in rows {
        let (
            json,
            antigravity_id,
            last_used,
            created_at,
            quota_remaining,
            quota_updated_at,
            quota_details,
        ) = row.map_err(err)?;
        accounts.push(GoogleAccount {
            metadata: serde_json::from_str(&json).map_err(err)?,
            antigravity_id,
            last_used,
            created_at,
            quota_remaining,
            quota_updated_at,
            quota_details,
        });
    }
    let mut stmt=conn.prepare("SELECT id,name,description,notes,status,created_at,updated_at FROM google_projects ORDER BY updated_at DESC,id").map_err(err)?;
    let mut projects = stmt
        .query_map([], |r| {
            Ok(GoogleProject {
                metadata: GoogleProjectInput {
                    id: Some(r.get(0)?),
                    name: r.get(1)?,
                    description: r.get(2)?,
                    notes: r.get(3)?,
                    status: r.get(4)?,
                    assignments: vec![],
                },
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    for project in &mut projects {
        let mut stmt=conn.prepare("SELECT account_id,role,services_json FROM google_assignments WHERE project_id=?1 ORDER BY account_id").map_err(err)?;
        let rows = stmt
            .query_map([&project.metadata.id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?;
        for row in rows {
            let (account_id, role, json) = row.map_err(err)?;
            project.metadata.assignments.push(GoogleAssignment {
                account_id,
                role,
                services: serde_json::from_str(&json).map_err(err)?,
            });
        }
    }
    let mut stmt=conn.prepare("SELECT id,account_id,service_id,metric,value,max_value,unit,recorded_at,source FROM google_usage ORDER BY recorded_at DESC,id DESC").map_err(err)?;
    let usage = stmt
        .query_map([], |r| {
            Ok(GoogleUsageRecord {
                id: r.get(0)?,
                account_id: r.get(1)?,
                service_id: r.get(2)?,
                metric: r.get(3)?,
                value: r.get(4)?,
                max_value: r.get(5)?,
                unit: r.get(6)?,
                recorded_at: r.get(7)?,
                source: r.get(8)?,
            })
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    let services = services(conn)?;
    let mut stmt=conn.prepare("SELECT id,kind,account_id,project_id,title,detail,created_at FROM google_activity ORDER BY created_at DESC,id DESC LIMIT 1000").map_err(err)?;
    let activity = stmt
        .query_map([], |r| {
            Ok(GoogleActivity {
                id: r.get(0)?,
                kind: r.get(1)?,
                account_id: r.get(2)?,
                project_id: r.get(3)?,
                title: r.get(4)?,
                detail: r.get(5)?,
                created_at: r.get(6)?,
            })
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(GoogleSnapshot {
        accounts,
        projects,
        usage,
        services,
        activity,
        preferences: preferences(conn)?,
        backups: vec![],
    })
}

pub fn services(conn: &Connection) -> Result<Vec<GoogleService>> {
    let mut result = vec![];
    for &(id, name, url) in SERVICES {
        let(favorite,last_account_id)=conn.query_row("SELECT favorite,last_account_id FROM google_service_preferences WHERE service_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
        let mut stmt=conn.prepare("SELECT account_id,status,updated_at FROM google_service_status WHERE service_id=?1 ORDER BY account_id").map_err(err)?;
        let statuses = stmt
            .query_map([id], |r| {
                Ok(GoogleServiceStatus {
                    account_id: r.get(0)?,
                    status: r.get(1)?,
                    updated_at: r.get(2)?,
                })
            })
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        result.push(GoogleService {
            id: id.into(),
            name: name.into(),
            url: url.into(),
            favorite,
            last_account_id,
            statuses,
        });
    }
    Ok(result)
}
