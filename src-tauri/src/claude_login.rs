use crate::{
    account_profiles::{self, ClaudeStatus, Provider},
    error::{HubError, Result},
    signin_browser::{LoginPage, LoginProvider},
    HubState,
};
use serde::Serialize;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

pub(crate) struct Pending {
    child: Child,
    profile: Option<PathBuf>,
    page: LoginPage,
    id: String,
    started: Instant,
}

impl Drop for Pending {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginInfo {
    login_id: String,
    browser_opened: bool,
}

fn fail(message: &str) -> HubError {
    HubError::Operation(message.into())
}

const BROWSER_ADDR: &str = "AI_USAGE_HUB_CLAUDE_BROWSER_ADDR";
const BROWSER_NONCE: &str = "AI_USAGE_HUB_CLAUDE_BROWSER_NONCE";

fn authorization_url(value: &str) -> Option<String> {
    let url = reqwest::Url::parse(value.trim_matches('"')).ok()?;
    if url.scheme() != "https"
        || !matches!(
            (url.host_str(), url.path()),
            (Some("claude.com"), "/cai/oauth/authorize")
                | (
                    Some("claude.ai"),
                    "/oauth/authorize" | "/cai/oauth/authorize"
                )
        )
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
    let redirect = reqwest::Url::parse(query.get("redirect_uri")?).ok()?;
    if redirect.scheme() != "http"
        || !matches!(redirect.host_str(), Some("localhost" | "127.0.0.1"))
        || redirect.port().is_none()
        || redirect.path() != "/callback"
        || !redirect.username().is_empty()
        || redirect.password().is_some()
        || redirect.query().is_some()
        || redirect.fragment().is_some()
        || query.get("response_type").is_none_or(|v| v != "code")
        || query.get("state").is_none_or(|v| v.is_empty())
        || query.get("code_challenge").is_none_or(|v| v.is_empty())
    {
        return None;
    }
    Some(url.into())
}

// When Claude launches the Hub executable as its browser handler, forward only the validated
// authorization URL to the waiting Hub instance. No UI, OAuth callback code, or token is handled.
pub(crate) fn run_browser_adapter() -> Option<bool> {
    let address = std::env::var(BROWSER_ADDR).ok()?;
    let nonce = std::env::var(BROWSER_NONCE).unwrap_or_default();
    let result = (|| {
        let address: SocketAddr = address.parse().ok()?;
        if !address.ip().is_loopback()
            || nonce.is_empty()
            || !nonce
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return None;
        }
        let url = authorization_url(&std::env::args().nth(1)?)?;
        let mut connection = TcpStream::connect_timeout(&address, Duration::from_secs(2)).ok()?;
        connection
            .set_write_timeout(Some(Duration::from_secs(2)))
            .ok()?;
        write!(connection, "{nonce}\n{url}").ok()?;
        Some(())
    })();
    Some(result.is_some())
}

#[cfg(windows)]
fn start_cli(profile: Option<&Path>, address: SocketAddr, nonce: &str) -> Result<Child> {
    use std::os::windows::process::CommandExt;
    let handler = std::env::current_exe()?;
    for executable in Provider::Claude.candidates() {
        let mut command = Command::new(executable);
        account_profiles::configure(&mut command, Provider::Claude, profile);
        command
            .args(["auth", "login", "--claudeai"])
            .env("BROWSER", &handler)
            .env(BROWSER_ADDR, address.to_string())
            .env(BROWSER_NONCE, nonce)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000);
        if let Ok(child) = command.spawn() {
            return Ok(child);
        }
    }
    Err(fail("Could not start Claude Code in link mode."))
}

#[cfg(not(windows))]
fn start_cli(_: Option<&Path>, _: SocketAddr, _: &str) -> Result<Child> {
    Err(fail(
        "Opening a profile from Hub is currently available on Windows.",
    ))
}

fn receive_url(listener: &TcpListener, child: &mut Child, nonce: &str) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((connection, _)) => {
                let _ = connection.set_read_timeout(Some(Duration::from_secs(1)));
                let mut message = String::new();
                if connection.take(8192).read_to_string(&mut message).is_ok() {
                    if let Some((key, raw)) = message.split_once('\n') {
                        if key == nonce {
                            if let Some(url) = authorization_url(raw) {
                                return Ok(url);
                            }
                        }
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if child.try_wait()?.is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(75));
            }
            Err(err) => return Err(err.into()),
        }
    }
    Err(fail(
        "Claude Code did not provide a sign-in link. Check that Claude Code is up to date.",
    ))
}

fn begin(profile: Option<PathBuf>, language: String) -> Result<Pending> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let nonce = crate::proxy::generate_api_key();
    let mut child = start_cli(profile.as_deref(), listener.local_addr()?, &nonce)?;
    let setup = (|| {
        let url = receive_url(&listener, &mut child, &nonce)?;
        use sha2::{Digest, Sha256};
        let id = format!("{:x}", Sha256::digest(url.as_bytes()));
        let page = LoginPage::start(&id, url, language, LoginProvider::Claude)?;
        Ok((id, page))
    })();
    match setup {
        Ok((id, page)) => Ok(Pending {
            child,
            profile,
            page,
            id,
            started: Instant::now(),
        }),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
}

async fn operate<T: Send + 'static>(
    app: AppHandle,
    action: impl FnOnce(&mut Option<Pending>) -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<HubState>();
        let mut pending = state
            .claude_login
            .lock()
            .map_err(|_| fail("Claude Code sign-in is busy."))?;
        if pending
            .as_ref()
            .is_some_and(|login| login.started.elapsed() > LOGIN_TIMEOUT)
        {
            pending.take();
            return Err(fail("Claude Code sign-in timed out. Start a new sign-in."));
        }
        action(&mut pending)
    })
    .await
    .map_err(|_| fail("Claude Code status check stopped."))?
}

fn selected<'a>(pending: &'a mut Option<Pending>, id: &str) -> Result<&'a mut Pending> {
    pending
        .as_mut()
        .filter(|login| login.id == id)
        .ok_or_else(|| fail("That Claude Code sign-in is no longer pending."))
}

#[tauri::command]
pub(crate) async fn start_claude_profile_login(
    app: AppHandle,
    name: Option<String>,
    open_browser: bool,
) -> Result<LoginInfo> {
    let profile = account_profiles::resolve(&app, Provider::Claude, name.as_deref())?;
    let language = crate::signin_page::language(app.state::<HubState>().inner());
    operate(app, move |pending| {
        if pending.is_some() {
            return Err(fail(
                "Finish or close the current Claude Code sign-in first.",
            ));
        }
        if account_profiles::claude_status(profile.as_deref())?.logged_in {
            return Err(fail(
                "Create a new account profile to add a second Claude account.",
            ));
        }
        let login = begin(profile, language)?;
        let browser_opened =
            open_browser && crate::signin_page::open_browser(&login.page.url).is_ok();
        let login_id = login.id.clone();
        *pending = Some(login);
        Ok(LoginInfo {
            login_id,
            browser_opened,
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn get_claude_login_url(app: AppHandle, login_id: String) -> Result<String> {
    operate(app, move |pending| {
        Ok(selected(pending, &login_id)?.page.url.clone())
    })
    .await
}

#[tauri::command]
pub(crate) async fn copy_claude_login_url(app: AppHandle, login_id: String) -> Result<()> {
    operate(app, move |pending| {
        crate::signin_page::copy_url(&selected(pending, &login_id)?.page.url)
    })
    .await
}

#[tauri::command]
pub(crate) async fn open_claude_login(app: AppHandle, login_id: String) -> Result<()> {
    operate(app, move |pending| {
        crate::signin_page::open_browser(&selected(pending, &login_id)?.page.url)
    })
    .await
}

#[tauri::command]
pub(crate) async fn poll_claude_login(
    app: AppHandle,
    login_id: String,
) -> Result<Option<ClaudeStatus>> {
    operate(app, move |pending| {
        let login = selected(pending, &login_id)?;
        let Some(exit) = login.child.try_wait()? else {
            return Ok(None);
        };
        let login = pending.take().expect("selected login is pending");
        if !exit.success() {
            return Err(fail("Claude Code sign-in failed or was cancelled."));
        }
        let status = account_profiles::claude_status(login.profile.as_deref())?;
        if !status.logged_in || status.auth_method.as_deref() != Some("claude.ai") {
            return Err(fail("Claude Code did not confirm a saved sign-in."));
        }
        login.page.complete(true);
        Ok(Some(status))
    })
    .await
}

#[tauri::command]
pub(crate) async fn cancel_claude_login(app: AppHandle, login_id: String) -> Result<()> {
    operate(app, move |pending| {
        selected(pending, &login_id)?;
        pending.take();
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auth_link_accepts_only_claude_subscription_oauth_with_a_local_callback() {
        let query = "?response_type=code&redirect_uri=http%3A%2F%2Flocalhost%3A12345%2Fcallback&state=test&code_challenge=test";
        let valid = format!("https://claude.com/cai/oauth/authorize{query}");
        assert_eq!(
            authorization_url(&format!("\"{valid}\"")).as_deref(),
            Some(valid.as_str())
        );
        assert!(authorization_url(&format!(
            "https://claude.com.evil.test/cai/oauth/authorize{query}"
        ))
        .is_none());
        assert!(authorization_url(&valid.replace("localhost", "evil.test")).is_none());
        assert!(authorization_url(&valid.replace("callback", "elsewhere")).is_none());
    }
}
