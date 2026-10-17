//! In-app updates from the project's GitHub Releases.
//!
//! `check` compares the latest published release with the running version. `install` downloads
//! the Windows setup EXE, refuses it unless its SHA-256 matches the release's `SHA256SUMS.txt`,
//! starts it and lets the caller exit so the installer can replace the files. On macOS an update is
//! offered only when the release carries a `.dmg`, and installing opens the release page, because
//! an unsigned bundle has to be installed by hand.

use std::{io::Read, path::PathBuf, time::Duration};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{HubError, Result};

const REPOSITORY: &str = "hfatih/ai-usage-hub";
const CHECKSUMS_ASSET: &str = "SHA256SUMS.txt";

/// What the interface needs to offer an update.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    pub release_url: String,
    /// The installer for this system (`-setup.exe` on Windows, `.dmg` on macOS); `None` when the
    /// release has none.
    pub installer_name: Option<String>,
    #[serde(skip)]
    installer_url: Option<String>,
    #[serde(skip)]
    checksums_url: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn client(timeout: Duration) -> Result<Client> {
    Client::builder()
        .timeout(timeout)
        .user_agent(concat!("ai-usage-hub/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(operation)
}

/// Asks GitHub for the latest release and compares it with this build's version.
pub fn check() -> Result<UpdateInfo> {
    let release: Release = client(Duration::from_secs(12))?
        .get(format!("https://api.github.com/repos/{REPOSITORY}/releases/latest"))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(operation)?
        .json()
        .map_err(operation)?;
    Ok(info_from_release(release, env!("CARGO_PKG_VERSION"), cfg!(windows)))
}

/// `windows` picks the installer to look for. Elsewhere a release without a `.dmg` is not offered
/// at all: it holds only the Windows build, so pointing a Mac at it would be no update.
fn info_from_release(release: Release, current: &str, windows: bool) -> UpdateInfo {
    let latest = release.tag_name.trim_start_matches('v').to_owned();
    let suffix = if windows { "-setup.exe" } else { ".dmg" };
    let installer = release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(suffix) && is_release_download(&asset.browser_download_url));
    let checksums = release
        .assets
        .iter()
        .find(|asset| asset.name == CHECKSUMS_ASSET && is_release_download(&asset.browser_download_url));
    UpdateInfo {
        available: is_newer(&latest, current) && (windows || installer.is_some()),
        current_version: current.to_owned(),
        latest_version: latest,
        release_url: release.html_url,
        installer_name: installer.map(|asset| asset.name.clone()),
        installer_url: installer.map(|asset| asset.browser_download_url.clone()),
        checksums_url: checksums.map(|asset| asset.browser_download_url.clone()),
    }
}

/// Only files published under this repository's releases are ever downloaded.
fn is_release_download(url: &str) -> bool {
    url.starts_with(&format!("https://github.com/{REPOSITORY}/releases/download/"))
}

/// Numeric comparison of dotted versions (`0.1.10` > `0.1.9`); any pre-release suffix is ignored.
fn is_newer(latest: &str, current: &str) -> bool {
    let parts = |version: &str| -> Vec<u64> {
        version
            .split(['-', '+'])
            .next()
            .unwrap_or_default()
            .split('.')
            .map(|part| part.parse().unwrap_or(0))
            .collect()
    };
    parts(latest) > parts(current)
}

/// Windows: downloads and verifies the installer, then starts it. Returns `true` when the
/// installer was started and the app should exit. Elsewhere opens the release page and
/// returns `false`.
pub fn install() -> Result<bool> {
    let info = check()?;
    if !info.available {
        return Err(HubError::Operation(format!("AI Usage Hub {} is already the latest version", info.current_version)));
    }
    if !cfg!(windows) {
        open_in_browser(&info.release_url)?;
        return Ok(false);
    }
    let (Some(name), Some(url)) = (info.installer_name.as_deref(), info.installer_url.as_deref()) else {
        return Err(HubError::Operation("The latest release has no Windows installer".into()));
    };
    let checksums_url = info
        .checksums_url
        .as_deref()
        .ok_or_else(|| HubError::Operation("The latest release has no SHA256SUMS.txt, so the installer cannot be verified".into()))?;

    let http = client(Duration::from_secs(300))?;
    let checksums = download(&http, checksums_url)?;
    let expected = expected_checksum(&String::from_utf8_lossy(&checksums), name)
        .ok_or_else(|| HubError::Operation(format!("SHA256SUMS.txt lists no checksum for {name}")))?;
    let installer = download(&http, url)?;
    let actual = hex(&Sha256::digest(&installer));
    if !actual.eq_ignore_ascii_case(&expected) {
        return Err(HubError::Operation(format!("{name} failed its checksum check and was not run")));
    }

    let path = update_dir()?.join(name);
    std::fs::write(&path, &installer)?;
    tracing::info!(version = %info.latest_version, path = %path.display(), "starting verified update installer");
    std::process::Command::new(&path).spawn()?;
    Ok(true)
}

fn download(http: &Client, url: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    http.get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(operation)?
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The hash for `file` in a `sha256sum`-style list (`<hex>  <name>`, optional `*` before the name).
fn expected_checksum(list: &str, file: &str) -> Option<String> {
    list.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file && hash.len() == 64).then(|| hash.to_owned())
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn update_dir() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join("AI Usage Hub update");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn open_in_browser(url: &str) -> Result<()> {
    crate::platform::open_url(url)?;
    Ok(())
}

fn operation(error: impl std::fmt::Display) -> HubError {
    HubError::Operation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("0.1.25", "0.1.24"));
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("1.0.0", "0.9.99"));
        assert!(!is_newer("0.1.22", "0.1.24"));
        assert!(!is_newer("0.1.24", "0.1.24"));
    }

    #[test]
    fn checksum_list_is_matched_by_exact_file_name() {
        let list = "aaaa  other.exe\nABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789 *AI-Usage-Hub_0.1.25_x64-setup.exe\n";
        assert_eq!(
            expected_checksum(list, "AI-Usage-Hub_0.1.25_x64-setup.exe").as_deref(),
            Some("ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789")
        );
        assert_eq!(expected_checksum(list, "other.exe"), None, "a short hash is not a SHA-256");
    }

    #[test]
    fn macs_are_offered_only_releases_with_a_dmg() {
        let release = |assets: &[&str]| Release {
            tag_name: "v0.1.30".into(),
            html_url: "https://github.com/hfatih/ai-usage-hub/releases/tag/v0.1.30".into(),
            assets: assets
                .iter()
                .map(|name| Asset {
                    name: (*name).into(),
                    browser_download_url: format!("https://github.com/{REPOSITORY}/releases/download/v0.1.30/{name}"),
                })
                .collect(),
        };
        let windows_only = info_from_release(release(&["AI-Usage-Hub_0.1.30_x64-setup.exe"]), "0.1.24", false);
        assert!(!windows_only.available);
        let with_dmg = info_from_release(release(&["AI-Usage-Hub_0.1.30_x64-setup.exe", "AI-Usage-Hub_0.1.30_aarch64.dmg"]), "0.1.24", false);
        assert!(with_dmg.available);
        assert_eq!(with_dmg.installer_name.as_deref(), Some("AI-Usage-Hub_0.1.30_aarch64.dmg"));
    }

    #[test]
    fn only_this_repository_assets_are_offered() {
        let release = Release {
            tag_name: "v0.1.30".into(),
            html_url: "https://github.com/hfatih/ai-usage-hub/releases/tag/v0.1.30".into(),
            assets: vec![
                Asset { name: "Evil-setup.exe".into(), browser_download_url: "https://example.com/Evil-setup.exe".into() },
                Asset { name: "SHA256SUMS.txt".into(), browser_download_url: format!("https://github.com/{REPOSITORY}/releases/download/v0.1.30/SHA256SUMS.txt") },
            ],
        };
        let info = info_from_release(release, "0.1.24", true);
        assert!(info.available);
        assert_eq!(info.installer_name, None);
        assert!(info.checksums_url.is_some());
    }
}
