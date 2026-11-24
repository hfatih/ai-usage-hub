use crate::{
    database::Database,
    error::{HubError, Result},
    google_control::{
        self as store, GoogleAccountInput, GooglePreferences, GoogleProjectInput, GoogleSnapshot,
        GoogleUsageInput,
    },
    HubState,
};
use std::path::PathBuf;
use tauri::{Emitter, Manager};

fn operation<T>(value: std::result::Result<T, String>) -> Result<T> {
    value.map_err(HubError::Operation)
}

fn snapshot(database: &Database) -> Result<GoogleSnapshot> {
    let mut result = operation(store::snapshot(&database.connection))?;
    if database.path.is_some() {
        result.backups = operation(crate::google_backups::list(database))?;
    }
    Ok(result)
}

async fn run<T: Send + 'static>(
    app: tauri::AppHandle,
    action: impl FnOnce(&mut Database) -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<HubState>();
        let mut database = state
            .database
            .lock()
            .map_err(|_| HubError::Operation("The database is busy.".into()))?;
        action(&mut database)
    })
    .await
    .map_err(|_| HubError::Operation("The Google account operation stopped unexpectedly.".into()))?
}

#[tauri::command]
pub(crate) async fn google_control_snapshot(app: tauri::AppHandle) -> Result<GoogleSnapshot> {
    run(app, |database| snapshot(database)).await
}

#[tauri::command]
pub(crate) async fn google_save_account(
    app: tauri::AppHandle,
    account: GoogleAccountInput,
) -> Result<GoogleSnapshot> {
    run(app, move |database| {
        operation(store::save_account(&database.connection, account))?;
        snapshot(database)
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_remove_account(
    app: tauri::AppHandle,
    account_id: String,
) -> Result<GoogleSnapshot> {
    run(app, move |database| {
        operation(store::remove_account(&database.connection, &account_id))?;
        snapshot(database)
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_save_project(
    app: tauri::AppHandle,
    project: GoogleProjectInput,
) -> Result<GoogleSnapshot> {
    run(app, move |database| {
        operation(store::save_project(&database.connection, project))?;
        snapshot(database)
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_delete_project(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<GoogleSnapshot> {
    run(app, move |database| {
        operation(store::delete_project(&database.connection, &project_id))?;
        snapshot(database)
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_add_usage(
    app: tauri::AppHandle,
    record: GoogleUsageInput,
) -> Result<GoogleSnapshot> {
    run(app, move |database| {
        operation(store::add_usage(&database.connection, record))?;
        snapshot(database)
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_delete_usage(
    app: tauri::AppHandle,
    record_id: i64,
) -> Result<GoogleSnapshot> {
    run(app, move |database| {
        operation(store::delete_usage(&database.connection, record_id))?;
        snapshot(database)
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_launch_service(
    app: tauri::AppHandle,
    account_id: String,
    service_id: String,
) -> Result<GoogleSnapshot> {
    let root = app
        .path()
        .app_local_data_dir()
        .map_err(|_| HubError::Operation("The local application folder is unavailable.".into()))?;
    run(app, move |database| {
        operation(store::check_launch(
            &database.connection,
            &account_id,
            &service_id,
        ))?;
        operation(crate::google_browser::launch(
            &root,
            &account_id,
            &service_id,
        ))?;
        operation(store::record_launch(
            &database.connection,
            &account_id,
            &service_id,
        ))?;
        snapshot(database)
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_set_service_status(
    app: tauri::AppHandle,
    account_id: String,
    service_id: String,
    status: String,
) -> Result<GoogleSnapshot> {
    run(app, move |database| {
        operation(store::set_service_status(
            &database.connection,
            &account_id,
            &service_id,
            &status,
        ))?;
        snapshot(database)
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_favorite_service(
    app: tauri::AppHandle,
    service_id: String,
    favorite: bool,
) -> Result<GoogleSnapshot> {
    run(app, move |database| {
        operation(store::favorite_service(
            &database.connection,
            &service_id,
            favorite,
        ))?;
        snapshot(database)
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_save_preferences(
    app: tauri::AppHandle,
    preferences: GooglePreferences,
) -> Result<GoogleSnapshot> {
    run(app, move |database| {
        operation(store::save_preferences(
            &database.connection,
            preferences.clone(),
        ))?;
        operation(crate::google_backups::automatic(database, &preferences))?;
        snapshot(database)
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_export_metadata(
    app: tauri::AppHandle,
    format: String,
) -> Result<String> {
    run(app, move |database| {
        operation(store::export_metadata(&database.connection, &format))
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_import_metadata(
    app: tauri::AppHandle,
    data: String,
    format: String,
) -> Result<GoogleSnapshot> {
    run(app, move |database| {
        operation(store::import_metadata(&database.connection, &data, &format))?;
        snapshot(database)
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_create_backup(app: tauri::AppHandle) -> Result<GoogleSnapshot> {
    run(app, |database| {
        operation(crate::google_backups::create(database))?;
        snapshot(database)
    })
    .await
}

#[tauri::command]
pub(crate) async fn google_restore_backup(
    app: tauri::AppHandle,
    path: String,
) -> Result<GoogleSnapshot> {
    let result = run(app.clone(), move |database| {
        operation(crate::google_backups::restore(
            database,
            &PathBuf::from(path),
        ))?;
        snapshot(database)
    })
    .await;
    if result.is_ok() {
        let _ = app.emit("hub://status-changed", ());
    }
    result
}

/// Runs from the existing monitor thread. It does not launch browsers or authenticate accounts.
pub(crate) fn maintenance(state: &HubState) -> Result<()> {
    let database = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("The database is busy.".into()))?;
    let result = snapshot(&database)?;
    operation(crate::google_backups::automatic(
        &database,
        &result.preferences,
    ))?;
    if !result.preferences.notifications_enabled {
        return Ok(());
    }
    let today = chrono::Utc::now().date_naive();
    let language = database.settings()?.language;
    let tx = database.connection.unchecked_transaction()?;
    for account in result
        .accounts
        .iter()
        .filter(|account| account.enabled && !account.archived)
    {
        let Some(id) = account.id.as_deref() else {
            continue;
        };
        let Some(date) = account
            .expiration_date
            .as_deref()
            .and_then(|value| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        else {
            continue;
        };
        let days = (date - today).num_days();
        if days > result.preferences.expiration_warning_days.clamp(0, 365) {
            continue;
        }
        let period = if days < 0 {
            "expired".to_owned()
        } else {
            today.to_string()
        };
        let created = tx.execute(
            "INSERT OR IGNORE INTO google_expiry_notices(account_id,expiration,period) VALUES(?1,?2,?3)",
            rusqlite::params![id, date.to_string(), period],
        )?;
        if created == 0 {
            continue;
        }
        let title = if language == "tr" {
            if days < 0 {
                "Google aboneliğinin süresi doldu"
            } else {
                "Google aboneliği yakında sona eriyor"
            }
        } else if days < 0 {
            "Google subscription expired"
        } else {
            "Google subscription expiring"
        };
        let detail = format!("{} · {} · {}", account.name, account.email, date);
        tx.execute(
            "INSERT INTO activity_events(event_type,title,detail,severity,created_at,metadata_json) VALUES('google_subscription_expiry',?1,?2,?3,?4,?5)",
            rusqlite::params![title, detail, if days < 0 { "danger" } else { "warning" }, chrono::Utc::now().timestamp(), serde_json::json!({"googleAccountId":id}).to_string()],
        )?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subscription_notifications_are_deduplicated_and_configurable() {
        let state = HubState::new(Database::memory().unwrap());
        let input = |email: &str| -> GoogleAccountInput {
            serde_json::from_value(serde_json::json!({
                "id":null,"email":email,"name":"Owned fixture","color":"#dba8c2",
                "subscription":"Google AI Pro","activationDate":null,
                "expirationDate":chrono::Utc::now().date_naive().to_string(),
                "enabled":true,"archived":false,"favorite":false,"group":"","tags":[],"notes":""
            }))
            .unwrap()
        };
        {
            let database = state.database.lock().unwrap();
            store::save_account(&database.connection, input("first@example.test")).unwrap();
        }
        maintenance(&state).unwrap();
        maintenance(&state).unwrap();
        {
            let database = state.database.lock().unwrap();
            assert_eq!(database.notifications(10).unwrap().len(), 1);
            let mut preferences = store::preferences(&database.connection).unwrap();
            preferences.notifications_enabled = false;
            store::save_preferences(&database.connection, preferences).unwrap();
            store::save_account(&database.connection, input("second@example.test")).unwrap();
        }
        maintenance(&state).unwrap();
        assert_eq!(
            state
                .database
                .lock()
                .unwrap()
                .notifications(10)
                .unwrap()
                .len(),
            1
        );
    }
}
