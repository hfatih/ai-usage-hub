//! Applies a saved CLI login to the normal CLI configuration, then verifies its identity.
use crate::{
    account_profiles::{self, Provider},
    codex_client::Client,
    error::{HubError, Result},
    HubState,
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

fn fail(message: &str) -> HubError {
    HubError::Operation(message.into())
}

fn read_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 2 * 1024 * 1024
    {
        return Err(fail("The CLI sign-in file could not be read safely."));
    }
    Ok(Some(fs::read(path)?))
}

fn read_json(path: &Path) -> Result<Option<Value>> {
    read_bytes(path)?
        .map(|bytes| {
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|_| fail("The CLI sign-in file contains invalid JSON."))?;
            if !value.is_object() {
                return Err(fail("The CLI sign-in file contains invalid JSON."));
            }
            Ok(value)
        })
        .transpose()
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| fail("The CLI folder is unavailable."))?;
    fs::create_dir_all(parent)?;
    // Replace in the same directory so a failed write leaves the previous file intact.
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".hub-switch-{}-{unique}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn credential_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Codex => "auth.json",
        Provider::Claude => ".credentials.json",
    }
}

fn valid_login(provider: Provider, credentials: &Value) -> bool {
    let (tokens, access, refresh) = match provider {
        Provider::Codex => (&credentials["tokens"], "access_token", "refresh_token"),
        Provider::Claude => (&credentials["claudeAiOauth"], "accessToken", "refreshToken"),
    };
    [access, refresh]
        .iter()
        .all(|key| tokens[key].as_str().is_some_and(|value| !value.is_empty()))
}

fn has_stored_login(provider: Provider, credentials: &Value) -> bool {
    match provider {
        Provider::Codex => credentials
            .as_object()
            .is_some_and(|value| value.values().any(|value| !value.is_null())),
        Provider::Claude => valid_login(provider, credentials),
    }
}

fn identity_key(provider: Provider, credentials: &Value, metadata: &Value) -> String {
    let identity = match provider {
        Provider::Codex => credentials["tokens"]["account_id"].as_str(),
        Provider::Claude => metadata["oauthAccount"]["accountUuid"].as_str(),
    };
    identity.map(str::to_owned).unwrap_or_else(|| {
        let token = match provider {
            Provider::Codex => &credentials["tokens"]["refresh_token"],
            Provider::Claude => &credentials["claudeAiOauth"]["refreshToken"],
        };
        let value = token
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| credentials.to_string());
        format!("{:x}", Sha256::digest(value.as_bytes()))
    })
}

fn save_previous(
    provider: Provider,
    profiles: &Path,
    credentials: &Value,
    metadata: &Value,
) -> Result<String> {
    fs::create_dir_all(profiles)?;
    let key = identity_key(provider, credentials, metadata);
    let mut existing = None;
    for entry in fs::read_dir(profiles)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir()
            || !entry.file_name().to_string_lossy().starts_with("profile-")
        {
            continue;
        }
        let path = entry.path();
        let Ok(Some(saved)) = read_json(&path.join(credential_name(provider))) else {
            continue;
        };
        let Ok(saved_metadata) = read_json(&path.join(".claude.json")) else {
            continue;
        };
        let saved_metadata = saved_metadata.unwrap_or_else(|| json!({}));
        if has_stored_login(provider, &saved)
            && identity_key(provider, &saved, &saved_metadata) == key
        {
            existing = Some(path);
            break;
        }
    }
    let path = if let Some(path) = existing {
        path
    } else {
        let label = match provider {
            Provider::Codex => "ChatGPT",
            Provider::Claude => "Claude",
        };
        let mut number = 1;
        loop {
            let path = profiles.join(format!("profile-{label} {number}"));
            match fs::create_dir(&path) {
                Ok(()) => break path,
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => number += 1,
                Err(err) => return Err(err.into()),
            }
        }
    };
    let saved = match provider {
        Provider::Codex => credentials.clone(),
        Provider::Claude => {
            let mut value =
                read_json(&path.join(credential_name(provider)))?.unwrap_or_else(|| json!({}));
            value["claudeAiOauth"] = credentials["claudeAiOauth"].clone();
            value
        }
    };
    write_private(
        &path.join(credential_name(provider)),
        &serde_json::to_vec(&saved).unwrap(),
    )?;
    if matches!(provider, Provider::Claude) {
        let mut config = read_json(&path.join(".claude.json"))?.unwrap_or_else(|| json!({}));
        config["oauthAccount"] = metadata["oauthAccount"].clone();
        config["hasCompletedOnboarding"] = true.into();
        write_private(
            &path.join(".claude.json"),
            &serde_json::to_vec(&config).unwrap(),
        )?;
    }
    Ok(path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .strip_prefix("profile-")
        .unwrap()
        .to_owned())
}

fn restore(path: &Path, previous: Option<&[u8]>) -> Result<()> {
    match previous {
        Some(bytes) => write_private(path, bytes),
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        },
    }
}

fn switch_files(
    provider: Provider,
    source: &Path,
    default: &Path,
    metadata_path: &Path,
    profiles: &Path,
    verify: impl FnOnce() -> Result<()>,
) -> Result<Option<String>> {
    let filename = credential_name(provider);
    let target = read_json(&source.join(filename))?
        .filter(|value| valid_login(provider, value))
        .ok_or_else(|| fail("Complete sign-in for this account before switching."))?;
    let previous_bytes = read_bytes(&default.join(filename))?;
    let previous = read_json(&default.join(filename))?.unwrap_or_else(|| json!({}));
    let claude = matches!(provider, Provider::Claude);
    let metadata_bytes = if claude {
        read_bytes(metadata_path)?
    } else {
        None
    };
    let metadata = if claude {
        read_json(metadata_path)?
    } else {
        None
    }
    .unwrap_or_else(|| json!({}));
    let source_metadata = if claude {
        read_json(&source.join(".claude.json"))?
    } else {
        None
    }
    .unwrap_or_else(|| json!({}));
    let previous_profile = if has_stored_login(provider, &previous)
        && identity_key(provider, &previous, &metadata)
            != identity_key(provider, &target, &source_metadata)
    {
        Some(save_previous(provider, profiles, &previous, &metadata)?)
    } else {
        None
    };
    let new_credentials = if claude {
        let mut value = previous.clone();
        value["claudeAiOauth"] = target["claudeAiOauth"].clone();
        value
    } else {
        target
    };
    let mut new_metadata = metadata;
    if claude {
        new_metadata["oauthAccount"] = source_metadata["oauthAccount"].clone();
        new_metadata["hasCompletedOnboarding"] = true.into();
    }
    let written = serde_json::to_vec(&new_credentials).unwrap();
    let written_metadata = serde_json::to_vec(&new_metadata).unwrap();
    // Detect a CLI write that happened while the previous account was being archived.
    if read_bytes(&default.join(filename))? != previous_bytes
        || (claude && read_bytes(metadata_path)? != metadata_bytes)
    {
        return Err(fail(
            "The CLI account changed during switching. Refresh and try again.",
        ));
    }
    write_private(&default.join(filename), &written)?;
    let result = (|| {
        if claude {
            write_private(metadata_path, &written_metadata)?;
        }
        verify()
    })();
    if let Err(error) = result {
        // Preserve a newer external CLI update instead of overwriting it during rollback.
        let current_metadata = if claude {
            read_bytes(metadata_path)?
        } else {
            None
        };
        if read_bytes(&default.join(filename))?.as_deref() != Some(&written)
            || (claude
                && current_metadata.as_deref() != Some(&written_metadata)
                && current_metadata != metadata_bytes)
            || restore(&default.join(filename), previous_bytes.as_deref()).is_err()
            || (claude && restore(metadata_path, metadata_bytes.as_deref()).is_err())
        {
            return Err(fail("Switching failed and the CLI account could not be restored. The previous account is saved in its profile. Refresh before continuing."));
        }
        return Err(error);
    }
    Ok(previous_profile)
}

fn default_directory(provider: Provider) -> Result<(PathBuf, PathBuf)> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| fail("The user folder could not be found."))?;
    let (environment, folder) = match provider {
        Provider::Codex => ("CODEX_HOME", ".codex"),
        Provider::Claude => ("CLAUDE_CONFIG_DIR", ".claude"),
    };
    let configured = std::env::var_os(environment).filter(|value| !value.is_empty());
    let metadata = if matches!(provider, Provider::Claude) && configured.is_none() {
        home.join(".claude.json")
    } else {
        configured
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(folder))
            .join(".claude.json")
    };
    let directory = configured
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(folder));
    if !directory.is_absolute() {
        return Err(fail(
            "The CLI configuration directory must be an absolute path.",
        ));
    }
    Ok((directory, metadata))
}

#[derive(Debug)]
struct AccountIdentity {
    email: String,
    workspace: Option<String>,
}

impl AccountIdentity {
    fn matches(&self, other: &Self) -> bool {
        self.email.eq_ignore_ascii_case(&other.email) && self.workspace == other.workspace
    }
}

fn identity(provider: Provider, profile: Option<&Path>) -> Result<AccountIdentity> {
    match provider {
        Provider::Codex => {
            let mut client = Client::discover_profile(profile)?;
            let account = client.request("account/read", json!({"refreshToken":false}))?;
            let email = account["account"]["email"]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| fail("Complete sign-in for this account before switching."))?;
            let sessions = client.request(
                "accountSession/list",
                json!({"refreshWorkspaceMetadata":false}),
            )?;
            let active = sessions["activeSessionId"].as_str();
            let workspace = sessions["sessions"]
                .as_array()
                .and_then(|sessions| {
                    sessions.iter().find(|session| {
                        session["isActive"] == true
                            && session["sessionId"].as_str() == active
                            && session["email"]
                                .as_str()
                                .is_some_and(|value| value.eq_ignore_ascii_case(email))
                    })
                })
                .and_then(|session| session["selectedWorkspaceAccountId"].as_str())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    fail("The CLI did not confirm the selected account and workspace.")
                })?;
            Ok(AccountIdentity {
                email: email.to_owned(),
                workspace: Some(workspace.to_owned()),
            })
        }
        Provider::Claude => {
            let status = account_profiles::claude_status(profile)?;
            let email = status
                .email
                .filter(|value| status.logged_in && !value.is_empty())
                .ok_or_else(|| fail("Complete sign-in for this account before switching."))?;
            Ok(AccountIdentity {
                email,
                workspace: None,
            })
        }
    }
}

#[cfg(test)]
fn email(provider: Provider, profile: Option<&Path>) -> Result<String> {
    identity(provider, profile).map(|value| value.email)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SwitchResult {
    email: String,
    previous_profile: Option<String>,
}

#[tauri::command]
pub(crate) async fn switch_account_profile(
    app: tauri::AppHandle,
    provider: Provider,
    name: String,
    expected_email: String,
) -> Result<SwitchResult> {
    if expected_email.is_empty() || expected_email.len() > 320 {
        return Err(fail("Invalid account selection."));
    }
    let source = account_profiles::resolve(&app, provider, Some(&name))?
        .ok_or_else(|| fail("That account profile is no longer available."))?;
    let (default, metadata) = default_directory(provider)?;
    let profiles = account_profiles::profile_root(&app, provider)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<HubState>();
        // Hold this provider's controller for the full switch, including verification.
        let codex = if matches!(provider, Provider::Codex) {
            let guard = state.codex_accounts.lock().map_err(|_| fail("The account manager is busy."))?;
            guard.idle()?;
            Some(guard)
        } else { None };
        let claude = if matches!(provider, Provider::Claude) {
            let guard = state.claude_login.lock().map_err(|_| fail("The account manager is busy."))?;
            if guard.is_some() { return Err(fail("Finish or cancel the pending sign-in first.")); }
            Some(guard)
        } else { None };
        let _guards = (codex, claude);
        let source_identity = identity(provider, Some(&source))?;
        if !source_identity.email.eq_ignore_ascii_case(&expected_email) {
            return Err(fail("The saved account changed. Refresh and select it again."));
        }
        let previous_profile = switch_files(provider, &source, &default, &metadata, &profiles, || {
            if !identity(provider, None)?.matches(&source_identity) {
                return Err(fail("The CLI did not confirm the selected account. The previous account was restored."));
            }
            Ok(())
        })?;
        Ok(SwitchResult { email: source_identity.email, previous_profile })
    }).await.map_err(|_| fail("Account switching stopped unexpectedly."))?
}

include!("cli_switch_tests.rs");
