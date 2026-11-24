#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn codex(id: &str) -> Value {
        json!({"auth_mode":"chatgpt","tokens":{"account_id":id,"access_token":"fixture-access","refresh_token":"fixture-refresh","id_token":"fixture-id"}})
    }

    fn write(path: &Path, value: &Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    }

    #[test]
    fn switching_saves_previous_identity_and_applies_target() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let default = dir.path().join("default");
        let profiles = dir.path().join("profiles");
        write(&source.join("auth.json"), &codex("b"));
        write(&default.join("auth.json"), &codex("a"));
        let result = switch_files(
            Provider::Codex,
            &source,
            &default,
            &default.join(".claude.json"),
            &profiles,
            || {
                assert_eq!(
                    read_json(&default.join("auth.json")).unwrap().unwrap()["tokens"]["account_id"],
                    "b"
                );
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            read_json(
                &profiles
                    .join(format!("profile-{}", result.unwrap()))
                    .join("auth.json")
            )
            .unwrap()
            .unwrap(),
            codex("a")
        );
        assert_eq!(
            read_json(&source.join("auth.json")).unwrap().unwrap(),
            codex("b")
        );
    }

    #[test]
    fn failed_verification_restores_previous_account() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let default = dir.path().join("default");
        write(&source.join("auth.json"), &codex("b"));
        write(&default.join("auth.json"), &codex("a"));
        let result = switch_files(
            Provider::Codex,
            &source,
            &default,
            &default.join(".claude.json"),
            &dir.path().join("profiles"),
            || Err(fail("verification failed")),
        );
        assert!(result.is_err());
        assert_eq!(
            read_json(&default.join("auth.json")).unwrap().unwrap(),
            codex("a")
        );
    }

    #[test]
    fn claude_switch_preserves_mcp_and_settings_and_changes_account_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let default = dir.path().join("default");
        let metadata = dir.path().join("user.claude.json");
        let oauth = |token| json!({"accessToken":token,"refreshToken":"fixture-refresh","expiresAt":9999999999999_i64});
        write(
            &source.join(".credentials.json"),
            &json!({"claudeAiOauth":oauth("b"),"mcpOAuth":{"source":"private"}}),
        );
        write(
            &source.join(".claude.json"),
            &json!({"oauthAccount":{"accountUuid":"b","emailAddress":"b@example.test"},"hasCompletedOnboarding":true}),
        );
        write(
            &default.join(".credentials.json"),
            &json!({"claudeAiOauth":oauth("a"),"mcpOAuth":{"keep":"existing"}}),
        );
        write(
            &metadata,
            &json!({"oauthAccount":{"accountUuid":"a","emailAddress":"a@example.test"},"theme":"dark","projects":{"one":{}}}),
        );
        switch_files(
            Provider::Claude,
            &source,
            &default,
            &metadata,
            &dir.path().join("profiles"),
            || Ok(()),
        )
        .unwrap();
        let credentials = read_json(&default.join(".credentials.json"))
            .unwrap()
            .unwrap();
        assert_eq!(credentials["claudeAiOauth"]["accessToken"], "b");
        assert_eq!(credentials["mcpOAuth"], json!({"keep":"existing"}));
        let meta = read_json(&metadata).unwrap().unwrap();
        assert_eq!(meta["oauthAccount"]["accountUuid"], "b");
        assert_eq!(meta["theme"], "dark");
        assert!(meta["projects"]["one"].is_object());
    }

    #[test]
    fn signed_out_or_invalid_target_never_changes_default() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let default = dir.path().join("default");
        write(&source.join("auth.json"), &json!({"tokens":null}));
        write(&default.join("auth.json"), &codex("a"));
        assert!(switch_files(
            Provider::Codex,
            &source,
            &default,
            &default.join(".claude.json"),
            &dir.path().join("profiles"),
            || Ok(())
        )
        .is_err());
        assert_eq!(
            read_json(&default.join("auth.json")).unwrap().unwrap(),
            codex("a")
        );
    }

    #[test]
    fn repeated_switch_updates_existing_backup_instead_of_duplicating_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let default = dir.path().join("default");
        let profiles = dir.path().join("profiles");
        write(&source.join("auth.json"), &codex("b"));
        write(&default.join("auth.json"), &codex("a"));
        for _ in 0..2 {
            switch_files(
                Provider::Codex,
                &source,
                &default,
                &default.join(".claude.json"),
                &profiles,
                || Ok(()),
            )
            .unwrap();
            write(&default.join("auth.json"), &codex("a"));
        }
        assert_eq!(fs::read_dir(&profiles).unwrap().count(), 1);
    }

    #[test]
    fn refreshing_an_existing_claude_profile_preserves_its_mcp_logins() {
        let dir = tempfile::tempdir().unwrap();
        let profiles = dir.path().join("profiles");
        let saved = profiles.join("profile-Claude 1");
        let credentials =
            json!({"claudeAiOauth":{"accessToken":"current","refreshToken":"refresh"}});
        let metadata = json!({"oauthAccount":{"accountUuid":"same"}});
        write(
            &saved.join(".credentials.json"),
            &json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"refresh"},"mcpOAuth":{"keep":"existing"}}),
        );
        write(&saved.join(".claude.json"), &metadata);
        save_previous(Provider::Claude, &profiles, &credentials, &metadata).unwrap();
        assert_eq!(
            read_json(&saved.join(".credentials.json"))
                .unwrap()
                .unwrap()["mcpOAuth"],
            json!({"keep":"existing"})
        );
    }

    #[test]
    fn claude_verification_failure_restores_credentials_and_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let default = dir.path().join("default");
        let metadata = default.join(".claude.json");
        let oauth = |value| json!({"claudeAiOauth":{"accessToken":value,"refreshToken":"refresh"}});
        write(&source.join(".credentials.json"), &oauth("b"));
        write(
            &source.join(".claude.json"),
            &json!({"oauthAccount":{"accountUuid":"b"}}),
        );
        write(&default.join(".credentials.json"), &oauth("a"));
        let previous = json!({"oauthAccount":{"accountUuid":"a"},"projects":{"keep":true}});
        write(&metadata, &previous);
        assert!(switch_files(
            Provider::Claude,
            &source,
            &default,
            &metadata,
            &dir.path().join("profiles"),
            || Err(fail("fixture rejection"))
        )
        .is_err());
        assert_eq!(read_json(&metadata).unwrap().unwrap(), previous);
        assert_eq!(
            read_json(&default.join(".credentials.json"))
                .unwrap()
                .unwrap(),
            oauth("a")
        );
    }

    #[test]
    #[ignore = "requires installed CLIs and isolated fixture paths"]
    fn native_cli_roundtrip_reads_selected_accounts() {
        let fixtures = PathBuf::from(
            std::env::var_os("AI_USAGE_HUB_NATIVE_FIXTURES").expect("fixture directory"),
        );
        let scratch = tempfile::tempdir().unwrap();
        for (provider, label) in [(Provider::Codex, "codex"), (Provider::Claude, "claude")] {
            let default = scratch.path().join(label);
            let metadata = default.join(".claude.json");
            let profiles = scratch.path().join(format!("{label}-profiles"));
            let source_a = fixtures.join(label).join("a");
            let source_b = fixtures.join(label).join("b");
            fs::create_dir_all(&default).unwrap();
            if matches!(provider, Provider::Codex) {
                fs::write(
                    default.join("config.toml"),
                    "cli_auth_credentials_store = \"file\"\n",
                )
                .unwrap();
            }
            fs::copy(
                source_a.join(credential_name(provider)),
                default.join(credential_name(provider)),
            )
            .unwrap();
            if matches!(provider, Provider::Claude) {
                fs::copy(source_a.join(".claude.json"), &metadata).unwrap();
            }
            let native_email = || -> Result<String> {
                if matches!(provider, Provider::Codex) {
                    // Synthetic tokens exercise local native selection without authenticating with OpenAI.
                    let result = Client::discover_profile(Some(&default))?.request(
                        "accountSession/list",
                        json!({"refreshWorkspaceMetadata":false}),
                    )?;
                    result["sessions"]
                        .as_array()
                        .and_then(|sessions| sessions.iter().find(|item| item["isActive"] == true))
                        .and_then(|session| session["email"].as_str())
                        .map(str::to_owned)
                        .ok_or_else(|| fail("The native CLI did not read the fixture account."))
                } else {
                    email(provider, Some(&default))
                }
            };
            assert_eq!(
                native_email().unwrap(),
                "a@example.test",
                "{label} initial account"
            );
            for (source, expected) in [(&source_b, "b@example.test"), (&source_a, "a@example.test")]
            {
                switch_files(provider, source, &default, &metadata, &profiles, || {
                    assert_eq!(native_email()?, expected, "{label} selected account");
                    Ok(())
                })
                .unwrap();
            }
        }
    }

    #[test]
    fn same_account_switch_keeps_the_fresh_source_login() {
        let dir = tempfile::tempdir().unwrap();
        let profiles = dir.path().join("profiles");
        let source = profiles.join("profile-ChatGPT 2");
        let default = dir.path().join("default");
        let old = codex("same-account");
        let mut fresh = old.clone();
        fresh["tokens"]["access_token"] = "fixture-new-access".into();
        fresh["tokens"]["refresh_token"] = "fixture-new-refresh".into();
        write(&source.join("auth.json"), &fresh);
        write(&default.join("auth.json"), &old);
        switch_files(
            Provider::Codex,
            &source,
            &default,
            &default.join(".claude.json"),
            &profiles,
            || Ok(()),
        )
        .unwrap();
        assert_eq!(
            read_json(&default.join("auth.json")).unwrap().unwrap(),
            fresh
        );
        assert_eq!(
            read_json(&source.join("auth.json")).unwrap().unwrap(),
            fresh
        );
    }

    #[test]
    fn matching_email_with_another_workspace_is_rejected() {
        let selected = AccountIdentity {
            email: "user@example.test".into(),
            workspace: Some("team".into()),
        };
        let wrong = AccountIdentity {
            email: "user@example.test".into(),
            workspace: Some("personal".into()),
        };
        let correct = AccountIdentity {
            email: "USER@example.test".into(),
            workspace: Some("team".into()),
        };
        assert!(!selected.matches(&wrong));
        assert!(selected.matches(&correct));
    }

    #[test]
    fn switching_from_an_api_login_preserves_its_auth_file() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let default = dir.path().join("default");
        let profiles = dir.path().join("profiles");
        let previous =
            json!({"auth_mode":"apikey","OPENAI_API_KEY":"fixture-only-api-key","tokens":null});
        write(&source.join("auth.json"), &codex("chatgpt-account"));
        write(&default.join("auth.json"), &previous);
        let saved = switch_files(
            Provider::Codex,
            &source,
            &default,
            &default.join(".claude.json"),
            &profiles,
            || Ok(()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            read_json(&profiles.join(format!("profile-{saved}")).join("auth.json"))
                .unwrap()
                .unwrap(),
            previous
        );
        assert_eq!(
            read_json(&default.join("auth.json")).unwrap().unwrap(),
            codex("chatgpt-account")
        );
    }
}
