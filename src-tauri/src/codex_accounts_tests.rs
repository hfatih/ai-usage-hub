#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn sessions(active: &str, workspace: &str) -> Value {
        json!({"activeSessionId":active,"sessions":[
            {"sessionId":"a","email":"one@example.com","isActive":active == "a","lastUsedAt":1,
             "selectedWorkspaceAccountId":if active == "a" {workspace} else {"personal"},
             "workspaces":[{"accountId":"personal","name":"Personal"},{"accountId":"team","name":"Team"}]},
            {"sessionId":"b","email":"two@example.com","isActive":active == "b","lastUsedAt":2,
             "selectedWorkspaceAccountId":if active == "b" {workspace} else {"other"},
             "workspaces":[{"accountId":"other","name":"Personal"}]}
        ]})
    }

    #[test]
    fn codex_switch_checks_the_workspace_not_only_the_email() {
        let result = parse_sessions(sessions("a", "personal")).unwrap();
        assert!(verify_selection(&result, "a", "team").is_err());
        assert!(verify_selection(&result, "a", "personal").is_ok());
        assert!(verify_selection(&result, "b", "other").is_err());
    }

    #[test]
    fn codex_switch_rejects_unknown_workspace_before_mutation() {
        let mut calls = Vec::new();
        let mut rpc = |method: &str, _params: Value| {
            calls.push(method.to_owned());
            Ok(sessions("a", "personal"))
        };
        assert!(switch_verified(&mut rpc, "a", "missing").is_err());
        assert_eq!(calls, ["accountSession/list"]);
    }

    #[test]
    fn codex_switch_rolls_back_when_core_selects_the_wrong_workspace() {
        let mut calls = Vec::new();
        let mut rpc = |method: &str, params: Value| {
            calls.push((method.to_owned(), params));
            Ok(sessions("a", "personal"))
        };
        assert!(switch_verified(&mut rpc, "a", "team").is_err());
        let restores: Vec<_> = calls
            .iter()
            .filter(|(m, _)| m == "accountSession/switch")
            .collect();
        assert_eq!(restores.len(), 2);
        assert_eq!(restores[1].1["sessionId"], "a");
        assert_eq!(restores[1].1["accountId"], "personal");
    }

    #[test]
    fn codex_switch_verifies_core_selection_and_account_identity() {
        let mut rpc = |method: &str, _params: Value| {
            Ok(match method {
                "accountSession/list" => sessions("a", "personal"),
                "accountSession/switch" => sessions("b", "other"),
                "account/read" => json!({"account":{"type":"chatgpt","email":"two@example.com"}}),
                _ => panic!("unexpected method"),
            })
        };
        let selected = switch_verified(&mut rpc, "b", "other").unwrap();
        assert_eq!(selected.active_session_id.as_deref(), Some("b"));
    }

    #[test]
    fn codex_login_completion_ignores_another_login_and_reports_failure() {
        assert!(login_completion(&json!({"method":"account/login/completed","params":{"loginId":"other","success":true}}), "mine").is_none());
        assert!(login_completion(&json!({"method":"account/login/completed","params":{"loginId":"mine","success":false,"error":"secret upstream text"}}), "mine").unwrap().is_err());
        assert!(login_completion(
            &json!({"method":"account/login/completed","params":{"loginId":"mine","success":true}}),
            "mine"
        )
        .unwrap()
        .is_ok());
    }

    #[test]
    fn codex_login_url_rejects_other_origins_and_schemes() {
        assert!(validate_login_url("https://auth.openai.com/oauth/authorize?state=test").is_ok());
        for url in [
            "javascript:alert(1)",
            "https://auth.openai.com.evil.test/",
            "http://auth.openai.com/",
            "https://auth.openai.com@evil.test/",
        ] {
            assert!(validate_login_url(url).is_err());
        }
    }

    #[test]
    fn codex_hidden_sessions_store_identifiers_without_credentials() {
        let db = crate::database::Database::memory().unwrap();
        assert!(db.hidden_codex_sessions().unwrap().is_empty());
        db.hide_codex_session("session-a", true).unwrap();
        db.hide_codex_session("session-a", true).unwrap();
        assert_eq!(db.hidden_codex_sessions().unwrap(), ["session-a"]);
        assert!(!db.security_status().unwrap().provider_credentials_stored);
        db.hide_codex_session("session-a", false).unwrap();
        assert!(db.hidden_codex_sessions().unwrap().is_empty());
    }

    #[test]
    fn codex_login_finalization_restores_previous_selection_after_list_failure() {
        let mut calls = Vec::new();
        let mut rpc = |method: &str, params: Value| {
            calls.push((method.to_owned(), params));
            if method == "accountSession/list" {
                Err(failure("simulated timeout"))
            } else {
                Ok(sessions("a", "personal"))
            }
        };
        let previous = ("a".to_owned(), "personal".to_owned());
        assert!(finalize_login(&mut rpc, Some(&previous), &mut |_| Ok(())).is_err());
        assert_eq!(calls[1].0, "accountSession/switch");
        assert_eq!(calls[1].1["sessionId"], "a");
        assert_eq!(calls[1].1["accountId"], "personal");
    }

    #[test]
    fn codex_login_finalization_restores_previous_selection_after_database_failure() {
        let mut calls = Vec::new();
        let mut rpc = |method: &str, _params: Value| {
            calls.push(method.to_owned());
            Ok(if method == "accountSession/list" {
                sessions("b", "other")
            } else {
                sessions("a", "personal")
            })
        };
        let previous = ("a".to_owned(), "personal".to_owned());
        assert!(
            finalize_login(&mut rpc, Some(&previous), &mut |_| Err::<(), _>(failure(
                "database unavailable"
            )))
            .is_err()
        );
        assert_eq!(calls, ["accountSession/list", "accountSession/switch"]);
    }

    #[test]
    fn codex_login_finalization_reports_a_failed_rollback() {
        let mut rpc = |_method: &str, _params: Value| Err(failure("unavailable"));
        let previous = ("a".to_owned(), "personal".to_owned());
        let error = finalize_login(&mut rpc, Some(&previous), &mut |_| Ok(()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("previous account could not be restored"));
    }

    #[test]
    #[ignore = "read-only check requiring an installed signed-in Codex"]
    fn live_codex_saved_sessions_have_a_valid_active_workspace() {
        let sessions = list(&mut Client::discover().unwrap()).unwrap();
        let (session, workspace) = selection(&sessions).expect("signed-in Codex required");
        verify_selection(&sessions, &session, &workspace).unwrap();
    }
}
