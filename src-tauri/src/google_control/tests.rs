use super::*;

fn db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    conn
}
fn account(email: &str) -> GoogleAccountInput {
    GoogleAccountInput {
        id: None,
        email: email.into(),
        name: "Test".into(),
        color: "#aabbcc".into(),
        subscription: "Manual plan".into(),
        activation_date: Some("2026-01-01".into()),
        expiration_date: Some("2027-01-01".into()),
        enabled: true,
        archived: false,
        favorite: false,
        group: "Group".into(),
        tags: vec!["work".into()],
        notes: "Notes".into(),
    }
}

#[test]
fn account_crud_and_twenty_five_accounts() {
    let conn = db();
    for i in 0..25 {
        save_account(&conn, account(&format!("a{i}@example.com"))).unwrap();
    }
    let state = snapshot(&conn).unwrap();
    assert_eq!(state.accounts.len(), 25);
    let mut edit = state.accounts[0].metadata.clone();
    edit.name = "Edited".into();
    let id = edit.id.clone().unwrap();
    save_account(&conn, edit).unwrap();
    remove_account(&conn, &id).unwrap();
    let state = snapshot(&conn).unwrap();
    let archived = state
        .accounts
        .iter()
        .find(|a| a.metadata.id.as_ref() == Some(&id))
        .unwrap();
    assert!(archived.archived);
    assert!(!archived.enabled);
    assert_eq!(archived.name, "Edited");
    assert!(check_launch(&conn, &id, "gemini").is_err());
    assert_eq!(preferences(&conn).unwrap().backup_retention, 7);
}

#[test]
fn duplicate_email_is_case_insensitive_and_id_conflicts_fail() {
    let conn = db();
    save_account(&conn, account("Me@Example.com")).unwrap();
    save_account(&conn, account("me@example.com")).unwrap();
    assert_eq!(snapshot(&conn).unwrap().accounts.len(), 1);
    save_account(&conn, account("other@example.com")).unwrap();
    let mut edit = snapshot(&conn).unwrap().accounts[1].metadata.clone();
    edit.email = "ME@example.com".into();
    assert!(save_account(&conn, edit).is_err());
    assert_eq!(snapshot(&conn).unwrap().accounts.len(), 2);
}

#[test]
fn account_email_identity_is_immutable_and_failed_edit_preserves_metadata() {
    let conn = db();
    save_account(&conn, account("original@example.com")).unwrap();
    let mut input = snapshot(&conn).unwrap().accounts[0].metadata.clone();
    let before = serde_json::to_value(&input).unwrap();
    input.email = "different@example.com".into();
    input.name = "Must not persist".into();
    let error = save_account(&conn, input.clone()).unwrap_err();
    assert_eq!(error, "Add a separate account to use a different email.");
    let state = snapshot(&conn).unwrap();
    assert_eq!(state.accounts.len(), 1);
    assert_eq!(
        serde_json::to_value(&state.accounts[0].metadata).unwrap(),
        before
    );
    input.email = " ORIGINAL@EXAMPLE.COM ".into();
    input.name = "Allowed canonical edit".into();
    save_account(&conn, input).unwrap();
    assert_eq!(
        snapshot(&conn).unwrap().accounts[0].name,
        "Allowed canonical edit"
    );
}

#[test]
fn native_eligibility_defaults_true_and_obeys_email_and_id_linkage() {
    let conn = db();
    conn.execute_batch("CREATE TABLE antigravity_accounts(id TEXT PRIMARY KEY,email TEXT); INSERT INTO antigravity_accounts VALUES('native','ME@example.com');").unwrap();
    assert!(native_account_enabled(&conn, "native").unwrap());
    let mut input = account("me@example.com");
    input.enabled = false;
    save_account(&conn, input).unwrap();
    assert!(!native_account_enabled(&conn, "native").unwrap());
    let json: String = conn
        .query_row("SELECT metadata_json FROM google_accounts", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut input: GoogleAccountInput = serde_json::from_str(&json).unwrap();
    input.enabled = true;
    save_account(&conn, input.clone()).unwrap();
    assert!(native_account_enabled(&conn, "native").unwrap());
    input.archived = true;
    save_account(&conn, input).unwrap();
    assert!(!native_account_enabled(&conn, "native").unwrap());
    conn.execute("UPDATE google_accounts SET antigravity_id='alias'", [])
        .unwrap();
    assert!(!native_account_enabled(&conn, "alias").unwrap());
    assert!(native_account_enabled(&conn, "unlinked").unwrap());
}

#[test]
fn native_sync_preserves_manual_metadata_and_archive() {
    let conn = db();
    conn.execute_batch("CREATE TABLE antigravity_accounts(id TEXT PRIMARY KEY,email TEXT,display_name TEXT,tier TEXT,last_used INTEGER,quota_remaining_percent REAL,quota_details_json TEXT); INSERT INTO antigravity_accounts VALUES('ag1','ME@example.com','Native Name','Pro',100,75,'{\"remaining\":75,\"access_token\":\"secret\"}');").unwrap();
    save_account(&conn, account("me@example.com")).unwrap();
    let id = snapshot(&conn).unwrap().accounts[0]
        .metadata
        .id
        .clone()
        .unwrap();
    remove_account(&conn, &id).unwrap();
    let state = snapshot(&conn).unwrap();
    let a = &state.accounts[0];
    assert_eq!(a.name, "Test");
    assert_eq!(a.subscription, "Manual plan");
    assert!(a.archived);
    assert!(!a.enabled);
    assert_eq!(a.antigravity_id.as_deref(), Some("ag1"));
    assert_eq!(a.quota_remaining, Some(75.0));
    assert!(!a.quota_details.as_deref().unwrap().contains("secret"));
    assert_eq!(state.usage.len(), 1);
    assert_eq!(state.usage[0].source, "antigravity");
    assert_eq!(snapshot(&conn).unwrap().usage.len(), 1);
    conn.execute(
        "UPDATE antigravity_accounts SET quota_remaining_percent=60",
        [],
    )
    .unwrap();
    assert_eq!(snapshot(&conn).unwrap().usage.len(), 2);
    let output = export_metadata(&conn, "json").unwrap();
    assert!(!output.contains("secret"));
    assert!(!output.contains("quotaRemaining"));
    assert!(!output.contains("antigravityId"));
}

#[test]
fn json_import_validates_every_record_before_writing() {
    let conn = db();
    let valid = account("valid@example.com");
    let mut invalid = account("invalid@example.com");
    invalid.color = "red".into();
    let input = serde_json::json!({"version":1,"accounts":[valid,invalid]}).to_string();
    assert!(import_metadata(&conn, &input, "json").is_err());
    assert!(snapshot(&conn).unwrap().accounts.is_empty());
    let mut injection = serde_json::to_value(account("test@example.com")).unwrap();
    injection["accessToken"] = "secret".into();
    assert!(import_metadata(
        &conn,
        &serde_json::json!({"version":1,"accounts":[injection]}).to_string(),
        "json"
    )
    .is_err());
    let a = account("dup@example.com");
    let mut b = a.clone();
    b.email = "DUP@example.com".into();
    assert!(import_metadata(
        &conn,
        &serde_json::json!({"version":1,"accounts":[a,b]}).to_string(),
        "json"
    )
    .is_err());
}

#[test]
fn csv_round_trip_quotes_commas_unicode_and_newlines() {
    let source = db();
    let mut a = account("test@example.com");
    a.notes = "Satır bir, \"alıntı\"\r\nsatır iki\nson".into();
    a.name = "Türkçe, ad".into();
    save_account(&source, a.clone()).unwrap();
    let csv = export_metadata(&source, "csv").unwrap();
    let target = db();
    import_metadata(&target, &csv, "csv").unwrap();
    let state = snapshot(&target).unwrap();
    assert_eq!(state.accounts[0].notes, a.notes);
    assert_eq!(state.accounts[0].name, a.name);
    import_metadata(&target, &csv, "csv").unwrap();
    assert_eq!(snapshot(&target).unwrap().accounts.len(), 1);
    assert!(import_metadata(&target, "id,email\r\n1,wrong", "csv").is_err());
}

#[test]
fn import_rolls_back_database_failures() {
    let conn = db();
    conn.execute_batch("CREATE TRIGGER fail_second BEFORE INSERT ON google_accounts WHEN NEW.email='second@example.com' BEGIN SELECT RAISE(ABORT,'failure'); END;").unwrap();
    let input=serde_json::json!({"version":1,"accounts":[account("first@example.com"),account("second@example.com")]}).to_string();
    assert!(import_metadata(&conn, &input, "json").is_err());
    assert!(snapshot(&conn).unwrap().accounts.is_empty());
}

#[test]
fn project_crud_assignments_cascade_without_deleting_accounts() {
    let conn = db();
    save_account(&conn, account("test@example.com")).unwrap();
    let account_id = snapshot(&conn).unwrap().accounts[0]
        .metadata
        .id
        .clone()
        .unwrap();
    save_project(
        &conn,
        GoogleProjectInput {
            id: None,
            name: "Project".into(),
            description: String::new(),
            notes: String::new(),
            status: "active".into(),
            assignments: vec![GoogleAssignment {
                account_id: account_id.clone(),
                role: "Owner".into(),
                services: vec!["gemini".into(), "flow".into()],
            }],
        },
    )
    .unwrap();
    let state = snapshot(&conn).unwrap();
    let project = &state.projects[0];
    assert_eq!(project.metadata.assignments.len(), 1);
    let id = project.metadata.id.clone().unwrap();
    delete_project(&conn, &id).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM google_assignments", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(snapshot(&conn).unwrap().accounts.len(), 1);
    assert!(snapshot(&conn).unwrap().projects.is_empty());
}

#[test]
fn usage_is_manual_and_rejects_invalid_numbers_and_services() {
    let conn = db();
    save_account(&conn, account("test@example.com")).unwrap();
    let account_id = snapshot(&conn).unwrap().accounts[0]
        .metadata
        .id
        .clone()
        .unwrap();
    let mut record = GoogleUsageInput {
        account_id,
        service_id: "gemini".into(),
        metric: "requests".into(),
        value: 3.0,
        max_value: Some(100.0),
        unit: "requests".into(),
        recorded_at: Some(1_000),
    };
    add_usage(&conn, record.clone()).unwrap();
    let state = snapshot(&conn).unwrap();
    assert_eq!(state.usage[0].source, "manual");
    assert_eq!(state.usage[0].recorded_at, 1_000);
    delete_usage(&conn, state.usage[0].id).unwrap();
    record.value = f64::NAN;
    assert!(add_usage(&conn, record.clone()).is_err());
    record.value = 3.0;
    record.service_id = "https://evil.example".into();
    assert!(add_usage(&conn, record).is_err());
}
