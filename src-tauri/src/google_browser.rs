//! Persistent, account-specific browser profiles. Authentication stays in the browser.
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) fn profile_directory(root: &Path, account: &str) -> Result<PathBuf, String> {
    if account.is_empty()
        || account.len() > 128
        || !account
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return Err("Invalid Google account selection.".into());
    }
    fs::create_dir_all(root).map_err(|_| "The browser profile folder could not be created.")?;
    let root_metadata =
        fs::symlink_metadata(root).map_err(|_| "The browser profile folder is unavailable.")?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("The browser profile folder is not a regular directory.".into());
    }
    let path = root.join(account);
    match fs::create_dir(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("The account browser profile could not be created.".into()),
    }
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| "The account browser profile is unavailable.")?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("The account browser profile is not a regular directory.".into());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|_| "The browser profile folder is unavailable.")?;
    let canonical = path
        .canonicalize()
        .map_err(|_| "The account browser profile is unavailable.")?;
    if canonical.parent() != Some(canonical_root.as_path()) {
        return Err("The account browser profile is outside the managed folder.".into());
    }
    Ok(canonical)
}

fn profile_for_browser(verified: &Path) -> Result<PathBuf, String> {
    #[cfg(windows)]
    let browser_path = verified
        .to_str()
        .and_then(|path| path.strip_prefix(r"\\?\"))
        .filter(|path| path.as_bytes().get(1) == Some(&b':'))
        .map(PathBuf::from)
        .unwrap_or_else(|| verified.to_path_buf());
    #[cfg(not(windows))]
    let browser_path = verified.to_path_buf();
    // Chrome's persistent profile uses the standard Windows drive-path form. The
    // normalized argument must resolve to the exact directory already verified above.
    let resolved = browser_path
        .canonicalize()
        .map_err(|_| "The verified browser profile is no longer available.")?;
    if resolved != verified {
        return Err("The browser profile changed before launch.".into());
    }
    Ok(browser_path)
}

fn browser_candidates() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut paths = Vec::new();
        for variable in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
            if let Some(root) = std::env::var_os(variable) {
                paths.push(PathBuf::from(&root).join("Google/Chrome/Application/chrome.exe"));
            }
        }
        for variable in ["PROGRAMFILES(X86)", "PROGRAMFILES", "LOCALAPPDATA"] {
            if let Some(root) = std::env::var_os(variable) {
                paths.push(PathBuf::from(root).join("Microsoft/Edge/Application/msedge.exe"));
            }
        }
        paths
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into(),
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge".into(),
        ]
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        std::env::var_os("PATH")
            .into_iter()
            .flat_map(|paths| {
                std::env::split_paths(&paths)
                    .flat_map(|path| {
                        [
                            "google-chrome",
                            "chromium",
                            "chromium-browser",
                            "microsoft-edge",
                        ]
                        .map(|name| path.join(name))
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

pub(crate) fn launch(app_data: &Path, account: &str, service: &str) -> Result<(), String> {
    let url = crate::google_control::service_url(service)?;
    let profile = profile_directory(&app_data.join("google-browser-profiles"), account)?;
    let executable = browser_candidates()
        .into_iter()
        .find(|path| path.is_absolute() && path.is_file())
        .ok_or("Install Google Chrome or Microsoft Edge to open isolated Google sessions.")?;
    let mut argument = OsString::from("--user-data-dir=");
    argument.push(profile_for_browser(&profile)?);
    let mut command = Command::new(executable);
    command.arg(argument).args([
        "--profile-directory=Default",
        "--new-window",
        "--no-first-run",
        "--no-default-browser-check",
        url,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "The account browser could not be started.")?;
    if child
        .try_wait()
        .map_err(|_| "The browser launch could not be checked.")?
        .is_some_and(|status| !status.success())
    {
        return Err("The account browser could not be started.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accounts_have_separate_persistent_profile_directories() {
        let directory = tempfile::tempdir().unwrap();
        let a = profile_directory(directory.path(), "account-a").unwrap();
        let b = profile_directory(directory.path(), "account-b").unwrap();
        assert_ne!(a, b);
        fs::write(a.join("session-marker"), "account-a").unwrap();
        assert_eq!(profile_directory(directory.path(), "account-a").unwrap(), a);
        assert!(!b.join("session-marker").exists());
    }
    #[test]
    fn account_identifiers_cannot_escape_the_profile_root() {
        let directory = tempfile::tempdir().unwrap();
        for account in ["", "../other", "..", "a/b", "a\\b", "a:stream", "/absolute"] {
            assert!(
                profile_directory(directory.path(), account).is_err(),
                "{account}"
            );
        }
    }

    #[test]
    #[ignore = "requires installed Chromium browser and the owned local fixture script"]
    fn native_browser_profiles_isolate_and_persist_test_cookies() {
        let directory = tempfile::tempdir().unwrap();
        let first = profile_directory(directory.path(), "account-a").unwrap();
        let second = profile_directory(directory.path(), "account-b").unwrap();
        let node = std::env::var_os("AI_USAGE_HUB_TEST_NODE").expect("Node executable");
        let script = std::env::var_os("AI_USAGE_HUB_BROWSER_TEST_SCRIPT")
            .expect("Owned local fixture script");
        let browser = browser_candidates()
            .into_iter()
            .find(|path| path.is_absolute() && path.is_file())
            .expect("Installed Chromium browser");
        let mut command = Command::new(node);
        command
            .arg(script)
            .arg(browser)
            .arg(profile_for_browser(&first).unwrap())
            .arg(profile_for_browser(&second).unwrap());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        assert!(command.status().unwrap().success());
    }

    #[test]
    fn browser_argument_keeps_the_verified_directory_identity() {
        let directory = tempfile::tempdir().unwrap();
        let verified = profile_directory(directory.path(), "account-a").unwrap();
        let argument = profile_for_browser(&verified).unwrap();
        assert_eq!(argument.canonicalize().unwrap(), verified);
        assert!(profile_for_browser(&directory.path().join("missing-profile")).is_err());
    }
}
