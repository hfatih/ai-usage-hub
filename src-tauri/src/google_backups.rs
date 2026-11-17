//! Consistent local SQLite backups and restoration into the existing managed connection.
use crate::{
    database::Database,
    google_control::{GoogleBackup, GooglePreferences},
};
use chrono::Utc;
use rusqlite::{
    backup::{Backup, StepResult},
    Connection, OpenFlags,
};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

fn backup_directory(database: &Database) -> Result<PathBuf, String> {
    let path = database
        .path
        .as_ref()
        .and_then(|path| path.parent())
        .ok_or("The database backup folder is unavailable.")?
        .join("backups");
    fs::create_dir_all(&path).map_err(|_| "The database backup folder could not be created.")?;
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| "The database backup folder is unavailable.")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("The database backup folder is not a regular directory.".into());
    }
    path.canonicalize()
        .map_err(|_| "The database backup folder is unavailable.".into())
}

fn copy_database(source: &Connection, destination: &mut Connection) -> Result<(), String> {
    let backup =
        Backup::new(source, destination).map_err(|_| "The SQLite backup could not be started.")?;
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        match backup.step(256).map_err(|_| "The SQLite backup failed.")? {
            StepResult::Done => return Ok(()),
            StepResult::More | StepResult::Busy | StepResult::Locked => {
                if Instant::now() >= deadline {
                    return Err("The database is busy. Try the backup again.".into());
                }
                thread::sleep(Duration::from_millis(10));
            }
            _ => return Err("The SQLite backup returned an unsupported state.".into()),
        }
    }
}

pub(crate) fn list(database: &Database) -> Result<Vec<GoogleBackup>, String> {
    let root = backup_directory(database)?;
    let mut backups = Vec::new();
    for entry in fs::read_dir(&root).map_err(|_| "The database backups could not be listed.")? {
        let entry = entry.map_err(|_| "A backup file could not be read.")?;
        let metadata = entry
            .metadata()
            .map_err(|_| "A backup file could not be read.")?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry
            .file_type()
            .map_err(|_| "A backup file could not be read.")?
            .is_file()
            || !name.starts_with("ai-usage-hub-")
            || !name.ends_with(".sqlite3")
        {
            continue;
        }
        let created_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_secs() as i64)
            .unwrap_or(0);
        backups.push(GoogleBackup {
            name,
            path: entry.path().to_string_lossy().into_owned(),
            created_at,
            size: metadata.len(),
        });
    }
    backups.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.name.cmp(&a.name))
    });
    Ok(backups)
}

fn create_named(database: &Database, name: &str) -> Result<PathBuf, String> {
    let root = backup_directory(database)?;
    let path = root.join(name);
    if path.exists() {
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| "The existing backup could not be checked.")?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("The existing backup is not a regular file.".into());
        }
        let valid = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok().is_some_and(|connection| {
            let check = connection.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0)).ok();
            let has_accounts = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='accounts')", [], |row| row.get::<_, bool>(0)).unwrap_or(false);
            check.as_deref() == Some("ok") && has_accounts
        });
        if valid {
            return Ok(path);
        }
        fs::remove_file(&path)
            .map_err(|_| "The incomplete automatic backup could not be replaced.")?;
    }
    let result = (|| {
        let mut destination = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
        .map_err(|_| "The backup file could not be created.")?;
        copy_database(&database.connection, &mut destination)?;
        let integrity: String = destination
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(|_| "The database backup could not be verified.")?;
        if integrity != "ok" {
            return Err("The database backup failed its integrity check.".into());
        }
        Ok(path.clone())
    })();
    if result.is_err() {
        // Only this newly created, direct child of the verified backup directory is removed.
        let _ = fs::remove_file(&path);
    }
    result
}

pub(crate) fn create(database: &Database) -> Result<PathBuf, String> {
    create_named(
        database,
        &format!(
            "ai-usage-hub-manual-{}.sqlite3",
            Utc::now().format("%Y%m%d-%H%M%S-%f")
        ),
    )
}

pub(crate) fn automatic(
    database: &Database,
    preferences: &GooglePreferences,
) -> Result<(), String> {
    if !preferences.auto_backup || database.path.is_none() {
        return Ok(());
    }
    let name = format!("ai-usage-hub-auto-{}.sqlite3", Utc::now().format("%Y%m%d"));
    create_named(database, &name)?;
    let root = backup_directory(database)?;
    let mut automatic = list(database)?
        .into_iter()
        .filter(|backup| backup.name.starts_with("ai-usage-hub-auto-"))
        .collect::<Vec<_>>();
    automatic.sort_by(|a, b| b.name.cmp(&a.name));
    for backup in automatic
        .into_iter()
        .skip(preferences.backup_retention.max(1) as usize)
    {
        let path = PathBuf::from(backup.path);
        let canonical = path
            .canonicalize()
            .map_err(|_| "An expired backup could not be checked.")?;
        if canonical.parent() != Some(root.as_path()) {
            return Err("An expired backup is outside the managed folder.".into());
        }
        fs::remove_file(&path).map_err(|_| "An expired automatic backup could not be removed.")?;
    }
    Ok(())
}

pub(crate) fn restore(database: &mut Database, source_path: &Path) -> Result<(), String> {
    if !source_path.is_absolute() {
        return Err("Select an existing AI Usage Hub database backup.".into());
    }
    let source = Connection::open_with_flags(source_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| "The selected database backup could not be opened.")?;
    let integrity: String = source
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|_| "The selected file is not a valid SQLite backup.")?;
    if integrity != "ok" {
        return Err("The selected database backup failed its integrity check.".into());
    }
    {
        let mut check = source
            .prepare("PRAGMA foreign_key_check")
            .map_err(|_| "The backup references could not be checked.")?;
        let mut rows = check
            .query([])
            .map_err(|_| "The backup references could not be checked.")?;
        if rows
            .next()
            .map_err(|_| "The backup references could not be checked.")?
            .is_some()
        {
            return Err("The selected backup contains broken data references.".into());
        }
    }
    for table in [
        "accounts",
        "providers",
        "settings",
        "antigravity_accounts",
        "usage_snapshots",
    ] {
        let exists: bool = source
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |row| row.get(0),
            )
            .map_err(|_| "The selected database backup could not be checked.")?;
        if !exists {
            return Err("The selected database is not an AI Usage Hub backup.".into());
        }
    }
    if database
        .path
        .as_ref()
        .and_then(|path| path.canonicalize().ok())
        == source_path.canonicalize().ok()
    {
        return Err("Choose a backup file instead of the active database.".into());
    }
    let recovery = create(database)?;
    if let Err(error) = copy_database(&source, &mut database.connection) {
        return Err(error);
    }
    let ready = database
        .migrate()
        .and_then(|_| database.seed())
        .and_then(|_| {
            crate::google_control::snapshot(&database.connection)
                .map(|_| ())
                .map_err(crate::error::HubError::Operation)
        });
    if ready.is_err() {
        let previous = Connection::open_with_flags(&recovery, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| {
                "The restored schema was unsupported. Use the pre-restore backup to recover."
            })?;
        copy_database(&previous, &mut database.connection)?;
        return Err(
            "The restored schema was unsupported. The previous database was restored.".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sqlite_backup_restores_data_and_keeps_a_pre_restore_copy() {
        let directory = tempfile::tempdir().unwrap();
        let mut database = Database::open(&directory.path().join("hub.sqlite3")).unwrap();
        database
            .connection
            .execute(
                "INSERT INTO settings(key,value_json,updated_at) VALUES('backup-test','1',0)",
                [],
            )
            .unwrap();
        let saved = create(&database).unwrap();
        database
            .connection
            .execute(
                "UPDATE settings SET value_json='2' WHERE key='backup-test'",
                [],
            )
            .unwrap();
        restore(&mut database, &saved).unwrap();
        let value: String = database
            .connection
            .query_row(
                "SELECT value_json FROM settings WHERE key='backup-test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, "1");
        assert!(list(&database).unwrap().len() >= 2);
    }
    #[test]
    fn unrelated_database_cannot_replace_the_hub() {
        let directory = tempfile::tempdir().unwrap();
        let mut database = Database::open(&directory.path().join("hub.sqlite3")).unwrap();
        let foreign = directory.path().join("foreign.sqlite3");
        Connection::open(&foreign)
            .unwrap()
            .execute("CREATE TABLE other(id INTEGER)", [])
            .unwrap();
        assert!(restore(&mut database, &foreign).is_err());
        assert!(database.settings().is_ok());
    }
    #[test]
    fn incomplete_automatic_backup_is_rebuilt() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(&directory.path().join("hub.sqlite3")).unwrap();
        let root = backup_directory(&database).unwrap();
        let path = root.join(format!(
            "ai-usage-hub-auto-{}.sqlite3",
            Utc::now().format("%Y%m%d")
        ));
        fs::write(&path, b"incomplete backup").unwrap();
        automatic(&database, &GooglePreferences::default()).unwrap();
        let backup = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let _: i64 = backup
            .query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))
            .unwrap();
    }
    #[test]
    fn malformed_google_data_rolls_back_a_restore() {
        let directory = tempfile::tempdir().unwrap();
        let mut database = Database::open(&directory.path().join("hub.sqlite3")).unwrap();
        database
            .connection
            .execute("UPDATE google_preferences SET json='{}' WHERE id=1", [])
            .unwrap();
        let malformed = create(&database).unwrap();
        database
            .connection
            .execute(
                "UPDATE google_preferences SET json=?1 WHERE id=1",
                [serde_json::to_string(&GooglePreferences::default()).unwrap()],
            )
            .unwrap();
        database
            .connection
            .execute(
                "INSERT INTO settings(key,value_json,updated_at) VALUES('restore-marker','keep',0)",
                [],
            )
            .unwrap();
        assert!(restore(&mut database, &malformed).is_err());
        let marker: String = database
            .connection
            .query_row(
                "SELECT value_json FROM settings WHERE key='restore-marker'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(marker, "keep");
        assert!(crate::google_control::preferences(&database.connection).is_ok());
    }

    #[test]
    #[ignore = "requires an explicitly selected local Hub database and only migrates a temporary copy"]
    fn existing_hub_data_survives_google_control_migration() {
        let original =
            std::env::var_os("AI_USAGE_HUB_TEST_DATABASE").expect("Selected Hub database");
        let source =
            Connection::open_with_flags(PathBuf::from(original), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let copy = directory.path().join("hub.sqlite3");
        {
            let mut destination = Connection::open(&copy).unwrap();
            copy_database(&source, &mut destination).unwrap();
        }
        let counts = |connection: &Connection| -> Vec<i64> {
            [
                "accounts",
                "antigravity_accounts",
                "usage_snapshots",
                "settings",
            ]
            .iter()
            .map(|table| {
                connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .unwrap()
            })
            .collect()
        };
        let before = {
            let connection = Connection::open(&copy).unwrap();
            counts(&connection)
        };
        let database = Database::open(&copy).unwrap();
        let snapshot = crate::google_control::snapshot(&database.connection).unwrap();
        assert_eq!(counts(&database.connection), before);
        let native_emails: i64 = database
            .connection
            .query_row(
                "SELECT COUNT(DISTINCT lower(trim(email))) FROM antigravity_accounts",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(snapshot.accounts.len() as i64 >= native_emails);
        let exported =
            crate::google_control::export_metadata(&database.connection, "json").unwrap();
        let value: serde_json::Value = serde_json::from_str(&exported).unwrap();
        for account in value["accounts"].as_array().unwrap() {
            for key in [
                "access_token",
                "refresh_token",
                "accessToken",
                "refreshToken",
                "cookie",
                "password",
            ] {
                assert!(account.get(key).is_none());
            }
        }
        let saved = create(&database).unwrap();
        assert!(saved.is_file());
        println!(
            "Existing Hub migration preserved {:?}; linked {} Google accounts",
            before,
            snapshot.accounts.len()
        );
    }
}
