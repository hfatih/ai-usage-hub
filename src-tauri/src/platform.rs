//! Per-OS locations and shell helpers, so provider adapters can ask for "the folder where
//! desktop apps keep their settings" instead of reading Windows-only environment variables.

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .filter(|_| cfg!(windows))
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Where Electron apps (Claude Desktop, Antigravity, VS Code forks) keep their user data:
/// `%APPDATA%` on Windows, `~/Library/Application Support` on macOS, `$XDG_CONFIG_HOME`
/// (or `~/.config`) elsewhere.
pub fn app_data_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home_dir().map(|home| home.join("Library/Application Support"))
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".config")))
    }
}

/// Folder for the hub's own log and crash files.
pub fn log_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        env::var_os("LOCALAPPDATA").map(|root| PathBuf::from(root).join("AI Usage Hub"))
    } else if cfg!(target_os = "macos") {
        home_dir().map(|home| home.join("Library/Logs/AI Usage Hub"))
    } else {
        env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".local/state")))
            .map(|root| root.join("ai-usage-hub"))
    }
}

/// File name of a command-line tool on this OS (`codex` → `codex.exe` on Windows).
pub fn executable_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_owned()
    }
}

/// Folders where command-line tools are commonly installed on macOS and Linux. Apps opened from
/// Finder or a login item inherit only `/usr/bin:/bin:/usr/sbin:/sbin`, so Homebrew, npm and
/// installer-script binaries would otherwise not be found.
pub fn extra_bin_dirs() -> Vec<PathBuf> {
    if cfg!(windows) {
        return Vec::new();
    }
    let mut dirs = Vec::new();
    if let Some(home) = home_dir() {
        for relative in [".local/bin", ".npm-global/bin", ".bun/bin", ".opencode/bin", ".claude/local", ".volta/bin"] {
            dirs.push(home.join(relative));
        }
    }
    for absolute in ["/opt/homebrew/bin", "/usr/local/bin"] {
        dirs.push(PathBuf::from(absolute));
    }
    dirs
}

/// Adds [`extra_bin_dirs`] to this process's `PATH` so CLI tools (and the `node` that npm-installed
/// CLIs start through) resolve the same way they do in a terminal. Call once at startup, before
/// any thread is spawned.
pub fn extend_search_path() {
    let extra = extra_bin_dirs();
    if extra.is_empty() {
        return;
    }
    let mut paths: Vec<PathBuf> = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default();
    for dir in extra {
        if dir.is_dir() && !paths.contains(&dir) {
            paths.push(dir);
        }
    }
    if let Ok(joined) = env::join_paths(paths) {
        env::set_var("PATH", joined);
    }
}

/// Whether `path` is something this OS can launch: a `.exe` on Windows, an `.app` bundle or an
/// executable file on macOS, an executable file elsewhere.
pub fn is_launchable(path: &Path) -> bool {
    if cfg!(windows) {
        return path.is_file() && has_extension(path, "exe");
    }
    if cfg!(target_os = "macos") && path.is_dir() && has_extension(path, "app") {
        return true;
    }
    is_executable_file(path)
}

pub fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension().is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// Opens `url` in the default browser.
pub fn open_url(url: &str) -> std::io::Result<()> {
    let mut command = if cfg!(windows) {
        let mut command = Command::new("rundll32");
        command.args(["url.dll,FileProtocolHandler", url]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("/usr/bin/open");
        command.arg(url);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };
    command.spawn().map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_names_follow_the_platform() {
        let name = executable_name("codex");
        assert_eq!(name.ends_with(".exe"), cfg!(windows));
        assert!(name.starts_with("codex"));
    }

    #[cfg(unix)]
    #[test]
    fn only_files_with_an_execute_bit_are_launchable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("tool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!is_launchable(&tool));
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_launchable(&tool));
        assert!(!is_launchable(dir.path()));
    }
}
