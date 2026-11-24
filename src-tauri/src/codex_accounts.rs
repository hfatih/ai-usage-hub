//! Saved logins are owned by Codex; Hub stores only display preferences, never tokens.
use crate::{
    account_profiles::{self, Provider},
    codex_client::Client,
    database::Database,
    error::{HubError, Result},
    HubState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::Manager;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Workspace {
    pub account_id: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Session {
    pub session_id: String,
    pub email: Option<String>,
    pub is_active: bool,
    pub last_used_at: i64,
    pub selected_workspace_account_id: Option<String>,
    pub workspaces: Vec<Workspace>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Sessions {
    active_session_id: Option<String>,
    sessions: Vec<Session>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginInfo {
    login_id: String,
    browser_opened: bool,
}

struct PendingLogin {
    client: Client,
    id: String,
    page: crate::signin_browser::LoginPage,
    started: Instant,
    previous: Option<(String, String)>,
}

#[derive(Default)]
pub(crate) struct Controller {
    pending: Option<PendingLogin>,
}

fn failure(message: &str) -> HubError {
    HubError::Operation(message.into())
}

fn parse_sessions(value: Value) -> Result<Sessions> {
    serde_json::from_value(value).map_err(|_| {
        failure("Codex returned an invalid saved-account list. Update Codex and refresh.")
    })
}

fn list(client: &mut Client) -> Result<Sessions> {
    parse_sessions(client.request(
        "accountSession/list",
        json!({"refreshWorkspaceMetadata":false}),
    )?)
}

fn selection(sessions: &Sessions) -> Option<(String, String)> {
    sessions
        .sessions
        .iter()
        .find(|s| s.is_active && Some(&s.session_id) == sessions.active_session_id.as_ref())
        .and_then(|s| {
            s.selected_workspace_account_id
                .as_ref()
                .map(|id| (s.session_id.clone(), id.clone()))
        })
}

fn verify_selection(sessions: &Sessions, session_id: &str, account_id: &str) -> Result<()> {
    let valid = sessions.active_session_id.as_deref() == Some(session_id)
        && sessions.sessions.iter().filter(|s| s.is_active).count() == 1
        && sessions.sessions.iter().any(|s| {
            s.session_id == session_id
                && s.is_active
                && s.selected_workspace_account_id.as_deref() == Some(account_id)
                && s.workspaces.iter().any(|w| w.account_id == account_id)
        });
    if valid {
        Ok(())
    } else {
        Err(failure(
            "Codex did not select the requested account and workspace.",
        ))
    }
}

fn switch_params(session_id: &str, account_id: &str) -> Value {
    json!({"sessionId":session_id,"accountId":account_id,"allowBrowserLogin":false})
}

fn switch_verified(
    rpc: &mut impl FnMut(&str, Value) -> Result<Value>,
    session_id: &str,
    account_id: &str,
) -> Result<Sessions> {
    let before = parse_sessions(rpc(
        "accountSession/list",
        json!({"refreshWorkspaceMetadata":false}),
    )?)?;
    let target = before
        .sessions
        .iter()
        .find(|s| {
            s.session_id == session_id && s.workspaces.iter().any(|w| w.account_id == account_id)
        })
        .ok_or_else(|| {
            failure(
                "That saved Codex account or workspace is no longer available. Refresh the list.",
            )
        })?;
    let switched = (|| {
        let result = parse_sessions(rpc(
            "accountSession/switch",
            switch_params(session_id, account_id),
        )?)?;
        verify_selection(&result, session_id, account_id)?;
        let account = rpc("account/read", json!({"refreshToken":false}))?;
        if account["account"]["type"] != "chatgpt"
            || target
                .email
                .as_deref()
                .is_some_and(|email| account["account"]["email"].as_str() != Some(email))
        {
            return Err(failure("Codex account verification failed."));
        }
        Ok(result)
    })();
    match switched {
        Ok(result) => Ok(result),
        Err(_) => {
            if let Some((old_session, old_account)) = selection(&before) {
                let restored = rpc(
                    "accountSession/switch",
                    switch_params(&old_session, &old_account),
                )
                .and_then(parse_sessions)
                .and_then(|s| verify_selection(&s, &old_session, &old_account));
                if restored.is_err() {
                    return Err(failure("Codex switching failed and the previous account could not be restored. Check the active account in Codex before continuing."));
                }
                Err(failure("Codex switching failed. The previous account was restored; sign in again if required."))
            } else {
                Err(failure("Codex switching could not be verified. Refresh and check the active account in Codex before continuing."))
            }
        }
    }
}

fn login_completion(message: &Value, id: &str) -> Option<Result<()>> {
    if message["method"] != "account/login/completed"
        || message["params"]["loginId"].as_str() != Some(id)
    {
        return None;
    }
    Some(if message["params"]["success"] == true {
        Ok(())
    } else {
        Err(failure(
            "Codex sign-in failed or was cancelled. Try signing in again.",
        ))
    })
}

fn finalize_login<T>(
    rpc: &mut impl FnMut(&str, Value) -> Result<Value>,
    previous: Option<&(String, String)>,
    commit: &mut impl FnMut(Sessions) -> Result<T>,
) -> Result<T> {
    let result = (|| {
        let sessions = parse_sessions(rpc(
            "accountSession/list",
            json!({"refreshWorkspaceMetadata":false}),
        )?)?;
        let (session, workspace) = selection(&sessions).ok_or_else(|| {
            failure("Codex sign-in finished without an active account. Refresh the list.")
        })?;
        verify_selection(&sessions, &session, &workspace)?;
        commit(sessions)
    })();
    if result.is_err() {
        if let Some((session, workspace)) = previous {
            let restored = rpc("accountSession/switch", switch_params(session, workspace))
                .and_then(parse_sessions)
                .and_then(|s| verify_selection(&s, session, workspace));
            if restored.is_err() {
                return Err(failure("Codex sign-in failed and the previous account could not be restored. Check the active account in Codex before continuing."));
            }
            return Err(failure(
                "Codex sign-in could not be verified. The previous account was restored.",
            ));
        }
        return Err(failure("Codex sign-in could not be verified. Refresh and check the active account in Codex before continuing."));
    }
    result
}

fn validate_login_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| failure("Codex returned an invalid sign-in link."))?;
    if url.scheme() != "https"
        || url.host_str() != Some("auth.openai.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|p| p != 443)
    {
        return Err(failure("Codex returned an untrusted sign-in link."));
    }
    Ok(())
}

fn visible(sessions: Sessions, db: &Mutex<Database>) -> Result<Vec<Session>> {
    let hidden = db
        .lock()
        .map_err(|_| failure("Database is busy."))?
        .hidden_codex_sessions()?;
    Ok(sessions
        .sessions
        .into_iter()
        .filter(|s| s.is_active || !hidden.contains(&s.session_id))
        .collect())
}

impl Controller {
    pub(crate) fn idle(&self) -> Result<()> {
        if self.pending.is_some() {
            Err(failure("Finish or cancel the current Codex sign-in first."))
        } else {
            Ok(())
        }
    }

    fn accounts(&mut self, profile: Option<&Path>, db: &Mutex<Database>) -> Result<Vec<Session>> {
        visible(list(&mut Client::discover_profile(profile)?)?, db)
    }

    fn start(
        &mut self,
        profile: Option<&Path>,
        open_browser_on_start: bool,
        language: String,
    ) -> Result<LoginInfo> {
        self.idle()?;
        let mut client = Client::discover_profile(profile)?;
        let before = list(&mut client)?;
        let previous = selection(&before);
        if before.sessions.iter().any(|session| session.is_active) {
            return Err(failure(
                "Create a new account profile to add a second ChatGPT account.",
            ));
        }
        let response = client.request(
            "account/login/start",
            json!({"type":"chatgpt","appBrand":"codex","useHostedLoginSuccessPage":false}),
        )?;
        let id = response["loginId"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| failure("Codex returned an invalid login ID."))?
            .to_owned();
        let url = response["authUrl"]
            .as_str()
            .ok_or_else(|| failure("Codex returned no sign-in link."))?
            .to_owned();
        validate_login_url(&url)?;
        let page = crate::signin_browser::LoginPage::start(
            &id,
            url,
            language,
            crate::signin_browser::LoginProvider::Codex,
        )?;
        let browser_opened =
            open_browser_on_start && crate::signin_page::open_browser(&page.url).is_ok();
        self.pending = Some(PendingLogin {
            client,
            id: id.clone(),
            page,
            started: Instant::now(),
            previous,
        });
        Ok(LoginInfo {
            login_id: id,
            browser_opened,
        })
    }

    fn restore(pending: &mut PendingLogin) -> Result<()> {
        if let Some((session, account)) = &pending.previous {
            let current = list(&mut pending.client)?;
            if verify_selection(&current, session, account).is_err() {
                let restored = parse_sessions(
                    pending
                        .client
                        .request("accountSession/switch", switch_params(session, account))?,
                )?;
                verify_selection(&restored, session, account)?;
            }
        }
        Ok(())
    }

    fn cancel(&mut self, login_id: &str) -> Result<()> {
        if self.pending.as_ref().is_none_or(|p| p.id != login_id) {
            return Err(failure("That Codex sign-in is no longer pending."));
        }
        let mut pending = self.pending.take().unwrap();
        let cancelled = pending
            .client
            .request("account/login/cancel", json!({"loginId":pending.id}));
        let restored = Self::restore(&mut pending);
        if restored.is_err() {
            return Err(failure("Codex sign-in stopped, but the previous account could not be verified. Check the active account in Codex."));
        }
        cancelled.map(|_| ())
    }

    fn poll(&mut self, login_id: &str, db: &Mutex<Database>) -> Result<Option<Vec<Session>>> {
        if self.pending.as_ref().is_none_or(|p| p.id != login_id) {
            return Err(failure("That Codex sign-in is no longer pending."));
        }
        let mut pending = self.pending.take().unwrap();
        while let Some(message) = pending.client.notification() {
            if let Some(completed) = login_completion(&message, login_id) {
                if let Err(error) = completed {
                    Self::restore(&mut pending)?;
                    return Err(error);
                }
                let result = finalize_login(
                    &mut |method, params| pending.client.request(method, params),
                    pending.previous.as_ref(),
                    &mut |sessions| {
                        let session = sessions
                            .active_session_id
                            .as_deref()
                            .ok_or_else(|| failure("Codex has no active account."))?;
                        db.lock()
                            .map_err(|_| failure("Database is busy."))?
                            .hide_codex_session(session, false)?;
                        visible(sessions, db)
                    },
                );
                pending.page.complete(result.is_ok());
                return result.map(Some);
            }
        }
        if pending.started.elapsed() > Duration::from_secs(180) {
            self.pending = Some(pending);
            self.cancel(login_id)?;
            return Err(failure("Codex sign-in timed out. Start a new sign-in."));
        }
        self.pending = Some(pending);
        Ok(None)
    }

    fn switch(
        &mut self,
        profile: Option<&Path>,
        session: &str,
        account: &str,
        db: &Mutex<Database>,
    ) -> Result<Vec<Session>> {
        self.idle()?;
        if session.is_empty() || account.is_empty() || session.len() > 512 || account.len() > 512 {
            return Err(failure("Invalid Codex account selection."));
        }
        let mut client = Client::discover_profile(profile)?;
        let sessions = switch_verified(&mut |m, p| client.request(m, p), session, account)?;
        visible(sessions, db)
    }

    fn hide(
        &mut self,
        profile: Option<&Path>,
        session: &str,
        db: &Mutex<Database>,
    ) -> Result<Vec<Session>> {
        self.idle()?;
        let sessions = list(&mut Client::discover_profile(profile)?)?;
        let target = sessions
            .sessions
            .iter()
            .find(|s| s.session_id == session)
            .ok_or_else(|| failure("That Codex account is no longer available."))?;
        if target.is_active {
            return Err(failure(
                "Switch to another Codex account before hiding the active account.",
            ));
        }
        db.lock()
            .map_err(|_| failure("Database is busy."))?
            .hide_codex_session(session, true)?;
        visible(sessions, db)
    }
}

async fn operate<T: Send + 'static>(
    app: tauri::AppHandle,
    action: impl FnOnce(&mut Controller, &Mutex<Database>) -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<HubState>();
        let mut controller = state
            .codex_accounts
            .lock()
            .map_err(|_| failure("Codex account manager is busy."))?;
        action(&mut controller, &state.database)
    })
    .await
    .map_err(|_| failure("Codex account operation stopped unexpectedly."))?
}

#[tauri::command]
pub(crate) async fn get_codex_accounts(
    app: tauri::AppHandle,
    profile: Option<String>,
) -> Result<Vec<Session>> {
    let path = account_profiles::resolve(&app, Provider::Codex, profile.as_deref())?;
    operate(app, move |c, db| c.accounts(path.as_deref(), db)).await
}
#[tauri::command]
pub(crate) async fn start_codex_login(
    app: tauri::AppHandle,
    open_browser: Option<bool>,
    profile: Option<String>,
) -> Result<LoginInfo> {
    let path = account_profiles::resolve(&app, Provider::Codex, profile.as_deref())?;
    let language = crate::signin_page::language(app.state::<HubState>().inner());
    operate(app, move |c, _| {
        c.start(path.as_deref(), open_browser.unwrap_or(true), language)
    })
    .await
}
#[tauri::command]
pub(crate) async fn get_codex_login_url(app: tauri::AppHandle, login_id: String) -> Result<String> {
    operate(app, move |c, _| {
        let pending = c
            .pending
            .as_ref()
            .filter(|p| p.id == login_id)
            .ok_or_else(|| failure("That Codex sign-in is no longer pending."))?;
        Ok(pending.page.url.clone())
    })
    .await
}
#[tauri::command]
pub(crate) async fn copy_codex_login_url(app: tauri::AppHandle, login_id: String) -> Result<()> {
    let url = get_codex_login_url(app, login_id).await?;
    crate::signin_page::copy_url(&url)
}

#[tauri::command]
pub(crate) async fn open_codex_login(app: tauri::AppHandle, login_id: String) -> Result<()> {
    operate(app, move |c, _| {
        let pending = c
            .pending
            .as_ref()
            .filter(|p| p.id == login_id)
            .ok_or_else(|| failure("That Codex sign-in is no longer pending."))?;
        crate::signin_page::open_browser(&pending.page.url)
    })
    .await
}
#[tauri::command]
pub(crate) async fn poll_codex_login(
    app: tauri::AppHandle,
    login_id: String,
) -> Result<Option<Vec<Session>>> {
    operate(app, move |c, db| c.poll(&login_id, db)).await
}
#[tauri::command]
pub(crate) async fn cancel_codex_login(app: tauri::AppHandle, login_id: String) -> Result<()> {
    operate(app, move |c, _| c.cancel(&login_id)).await
}
#[tauri::command]
pub(crate) async fn switch_codex_account(
    app: tauri::AppHandle,
    session_id: String,
    account_id: String,
    profile: Option<String>,
) -> Result<Vec<Session>> {
    let path = account_profiles::resolve(&app, Provider::Codex, profile.as_deref())?;
    operate(app, move |c, db| {
        c.switch(path.as_deref(), &session_id, &account_id, db)
    })
    .await
}
#[tauri::command]
pub(crate) async fn hide_codex_account(
    app: tauri::AppHandle,
    session_id: String,
    profile: Option<String>,
) -> Result<Vec<Session>> {
    let path = account_profiles::resolve(&app, Provider::Codex, profile.as_deref())?;
    operate(app, move |c, db| c.hide(path.as_deref(), &session_id, db)).await
}

include!("codex_accounts_tests.rs");
