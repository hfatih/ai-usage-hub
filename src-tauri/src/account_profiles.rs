use crate::{
    error::{HubError, Result},
    providers,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Provider {
    Codex,
    Claude,
}

impl Provider {
    fn directory(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    pub(crate) fn candidates(self) -> Vec<PathBuf> {
        match self {
            Self::Codex => providers::openai::codex_candidates(),
            Self::Claude => providers::anthropic::claude_candidates(),
        }
    }
}

fn profile_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > 40
        || !name.chars().any(char::is_alphanumeric)
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
    {
        return Err(HubError::Operation(
            "Use up to 40 letters, numbers, spaces, hyphens or underscores for the profile name."
                .into(),
        ));
    }
    Ok(name)
}

pub(crate) fn profile_root(app: &AppHandle, provider: Provider) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|_| HubError::Operation("Could not find the local profile folder.".into()))?
        .join("account-profiles")
        .join(provider.directory()))
}

pub(crate) fn resolve(
    app: &AppHandle,
    provider: Provider,
    name: Option<&str>,
) -> Result<Option<PathBuf>> {
    let Some(name) = name else {
        return Ok(None);
    };
    let path = profile_root(app, provider)?.join(format!("profile-{}", profile_name(name)?));
    let metadata = fs::symlink_metadata(&path).ok();
    if metadata.is_some_and(|m| m.is_dir() && !m.file_type().is_symlink()) {
        Ok(Some(path))
    } else {
        Err(HubError::Operation(
            "That account profile is no longer available.".into(),
        ))
    }
}

pub(crate) fn configure(command: &mut Command, provider: Provider, profile: Option<&Path>) {
    if let Some(profile) = profile {
        match provider {
            Provider::Codex => {
                command
                    .env("CODEX_HOME", profile)
                    .env_remove("CODEX_SQLITE_HOME");
            }
            Provider::Claude => {
                command.env("CLAUDE_CONFIG_DIR", profile);
            }
        }
    }
}

#[tauri::command]
pub(crate) fn get_account_profiles(app: AppHandle, provider: Provider) -> Result<Vec<String>> {
    let entries = match fs::read_dir(profile_root(&app, provider)?) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error.into()),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.strip_prefix("profile-"))
            {
                if profile_name(name).is_ok_and(|valid| valid == name) {
                    names.push(name.to_owned());
                }
            }
        }
    }
    names.sort();
    Ok(names)
}

#[tauri::command]
pub(crate) fn create_account_profile(
    app: AppHandle,
    provider: Provider,
    name: String,
) -> Result<String> {
    let name = profile_name(&name)?;
    let root = profile_root(&app, provider)?;
    fs::create_dir_all(&root)?;
    let path = root.join(format!("profile-{name}"));
    #[cfg(unix)]
    let result = {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    };
    #[cfg(not(unix))]
    let result = fs::create_dir(path);
    match result {
        Ok(()) => Ok(name.to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(
            HubError::Operation("A profile with this name already exists.".into()),
        ),
        Err(error) => Err(error.into()),
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeStatus {
    pub(crate) logged_in: bool,
    pub(crate) email: Option<String>,
    subscription_type: Option<String>,
    pub(crate) auth_method: Option<String>,
}

fn wait_for_child(child: &mut Child, timeout: Duration) -> std::io::Result<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        thread::sleep(Duration::from_millis(80));
    }
}

pub(crate) fn claude_status(profile: Option<&Path>) -> Result<ClaudeStatus> {
    for executable in Provider::Claude.candidates() {
        let mut command = Command::new(executable);
        configure(&mut command, Provider::Claude, profile);
        command
            .args(["auth", "status", "--json"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let Ok(mut child) = command.spawn() else {
            continue;
        };
        if wait_for_child(&mut child, Duration::from_secs(8)).is_ok() {
            if let Ok(output) = child.wait_with_output() {
                if let Ok(status) = serde_json::from_slice(&output.stdout) {
                    return Ok(status);
                }
            }
        }
    }
    Err(HubError::Operation(
        "Could not read Claude Code sign-in status. Check that Claude Code is installed.".into(),
    ))
}

#[tauri::command]
pub(crate) async fn get_claude_profile_status(
    app: AppHandle,
    name: Option<String>,
) -> Result<ClaudeStatus> {
    let profile = resolve(&app, Provider::Claude, name.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || claude_status(profile.as_deref()))
        .await
        .map_err(|_| HubError::Operation("Claude Code status check stopped.".into()))?
}

#[cfg(windows)]
fn launch_console(
    provider: Provider,
    profile: Option<&Path>,
    directory: &Path,
) -> Result<std::process::Child> {
    use std::os::windows::process::CommandExt;
    let candidates = provider
        .candidates()
        .into_iter()
        .filter(|path| path.is_absolute() && path.is_file());
    let filename = match provider {
        Provider::Codex => "codex.exe",
        Provider::Claude => "claude.exe",
    };
    let on_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .filter(|path| path.is_absolute())
        .map(|path| path.join(filename))
        .filter(|path| path.is_file());
    for executable in candidates.chain(on_path) {
        let mut command = Command::new(executable);
        configure(&mut command, provider, profile);
        command.current_dir(directory).creation_flags(0x0000_0010);
        if let Ok(child) = command.spawn() {
            return Ok(child);
        }
    }
    Err(HubError::Operation(
        "Could not open the CLI. Check that it is installed.".into(),
    ))
}

#[tauri::command]
pub(crate) fn launch_account_profile(
    app: AppHandle,
    provider: Provider,
    name: Option<String>,
    directory: String,
) -> Result<()> {
    let profile = resolve(&app, provider, name.as_deref())?;
    let directory = PathBuf::from(directory);
    if !directory.is_absolute() || !directory.is_dir() {
        return Err(HubError::Operation(
            "Select an existing project folder.".into(),
        ));
    }
    #[cfg(windows)]
    {
        launch_console(provider, profile.as_deref(), &directory)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = profile;
        Err(HubError::Operation(
            "Opening a profile from Hub is currently available on Windows.".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names_are_single_path_components() {
        assert_eq!(profile_name("  İş hesabı  ").unwrap(), "İş hesabı");
        for value in ["../other", "C:\\bad", "a/b", "a:stream", "..", "", "___"] {
            assert!(profile_name(value).is_err());
        }
    }

    #[test]
    fn different_profiles_set_distinct_child_only_config_roots() {
        let mut codex = Command::new("codex");
        configure(&mut codex, Provider::Codex, Some(Path::new("account-a")));
        let mut claude = Command::new("claude");
        configure(&mut claude, Provider::Claude, Some(Path::new("account-b")));
        assert!(codex
            .get_envs()
            .any(|(k, v)| k == "CODEX_HOME" && v.unwrap() == "account-a"));
        assert!(claude
            .get_envs()
            .any(|(k, v)| k == "CLAUDE_CONFIG_DIR" && v.unwrap() == "account-b"));
    }
}
