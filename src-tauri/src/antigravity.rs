use std::{
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

use crate::{
    database::Database,
    error::{HubError, Result},
    models::{
        AntigravityAccount, AntigravityLiveSession, AntigravityQuotaDetails, Confidence, Severity,
        UsageSource, UsageValue,
    },
    HubState,
};

pub fn get_oauth_client_id() -> String {
    if let Ok(id) = std::env::var("ANTIGRAVITY_CLIENT_ID") {
        return id;
    }
    ["1071006060591-", "tmhssin2h21lcre235vtolojh4g403ep", ".apps.googleusercontent.com"].concat()
}

pub fn get_oauth_client_secret() -> String {
    if let Ok(sec) = std::env::var("ANTIGRAVITY_CLIENT_SECRET") {
        return sec;
    }
    ["GOCS", "PX-K58FWR486L", "dLJ1mLB8sXC4z6qDAf"].concat()
}

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const OAUTH_REDIRECT_URI: &str = "http://localhost:8045/auth/callback";
const TARGET_CREDENTIAL: &str = "gemini:antigravity";
const TARGET_USERNAME: &str = "antigravity";

/// The OS store Antigravity keeps its Google sign-in in, as named in messages.
#[cfg(windows)]
pub const CREDENTIAL_STORE: &str = "Windows Credential Manager";
#[cfg(target_os = "macos")]
pub const CREDENTIAL_STORE: &str = "macOS Keychain";
#[cfg(not(any(windows, target_os = "macos")))]
pub const CREDENTIAL_STORE: &str = "system keyring";

/// Sent to Google's quota endpoint, in the form Antigravity itself uses for this OS.
#[cfg(target_os = "macos")]
const CLIENT_USER_AGENT: &str = "antigravity/darwin";
#[cfg(not(target_os = "macos"))]
const CLIENT_USER_AGENT: &str = "antigravity/windows";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeyringTokenDetails {
    pub access_token: String,
    pub token_type: String,
    pub refresh_token: String,
    pub expiry: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
}

/// The Antigravity sign-in stored under `gemini:antigravity` in Windows Credential Manager.
///
/// Antigravity 2.32+ keeps the account email in `id_token_email` instead of an `id_token`, and
/// adds fields of its own (`writer_id`, `user_tier`, ...). Those land in `extra` and are written
/// back unchanged, so rewriting the token never strips what Antigravity put there.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeyringPayload {
    pub token: KeyringTokenDetails,
    pub auth_method: String,
    /// Email of the signed-in account (Antigravity 2.32+). Must name the account the token belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token_email: Option<String>,
    /// Fields this hub does not interpret, kept as Antigravity wrote them.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Stored fields that describe the signed-in account rather than the installation; dropped when
/// another account's token is written in their place.
const ACCOUNT_SCOPED_FIELDS: [&str; 3] = ["user_tier", "project_id", "region"];

/// Fields to keep from the `previous` stored sign-in when writing `email`'s token over it:
/// everything Antigravity added, minus the account-scoped ones when the account changes.
fn carried_fields(previous: Option<&KeyringPayload>, email: &str) -> serde_json::Map<String, serde_json::Value> {
    let Some(previous) = previous else {
        return serde_json::Map::new();
    };
    let mut extra = previous.extra.clone();
    let same_account = previous.id_token_email.as_deref().is_some_and(|old| old.eq_ignore_ascii_case(email));
    if !same_account {
        for key in ACCOUNT_SCOPED_FIELDS {
            extra.remove(key);
        }
    }
    extra
}

/// Email and display name of a stored sign-in, without a network call: the `id_token` claims
/// when present, else Antigravity 2.32's `id_token_email` (no display name there).
pub fn payload_identity(payload: &KeyringPayload) -> Option<(String, Option<String>)> {
    payload
        .token
        .id_token
        .as_deref()
        .and_then(parse_jwt_claims)
        .or_else(|| payload.id_token_email.clone().map(|email| (email, None)))
}

#[derive(Debug, Deserialize)]
pub struct TokenRefreshResponse {
    pub access_token: String,
    pub expires_in: i64,
    #[serde(default)]
    pub token_type: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
}

/// Win32 native credential manager calls for Windows
#[cfg(windows)]
mod system_cred {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    #[repr(C)]
    struct FILETIME {
        dw_low_date_time: u32,
        dw_high_date_time: u32,
    }

    #[repr(C)]
    struct CREDENTIALW {
        flags: u32,
        cred_type: u32,
        target_name: *const u16,
        comment: *const u16,
        last_written: FILETIME,
        credential_blob_size: u32,
        credential_blob: *const u8,
        persist: u32,
        attribute_count: u32,
        attributes: *const std::ffi::c_void,
        target_alias: *const u16,
        user_name: *const u16,
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn CredReadW(
            target_name: *const u16,
            type_: u32,
            flags: u32,
            credential: *mut *mut CREDENTIALW,
        ) -> i32;
        fn CredWriteW(credential: *const CREDENTIALW, flags: u32) -> i32;
        fn CredDeleteW(target_name: *const u16, type_: u32, flags: u32) -> i32;
        fn CredFree(buffer: *mut std::ffi::c_void);
    }

    pub fn read_credential(target: &str) -> Result<Option<String>, String> {
        let target_wide: Vec<u16> = std::ffi::OsStr::new(target)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut cred_ptr: *mut CREDENTIALW = ptr::null_mut();
        unsafe {
            let res = CredReadW(target_wide.as_ptr(), 1, 0, &mut cred_ptr);
            if res == 0 || cred_ptr.is_null() {
                return Ok(None);
            }
            let cred = &*cred_ptr;
            let blob = std::slice::from_raw_parts(
                cred.credential_blob,
                cred.credential_blob_size as usize,
            );
            let content = String::from_utf8_lossy(blob).to_string();
            CredFree(cred_ptr as *mut std::ffi::c_void);
            Ok(Some(content))
        }
    }

    pub fn write_credential(target: &str, user: &str, secret: &[u8]) -> Result<(), String> {
        let target_wide: Vec<u16> = std::ffi::OsStr::new(target)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let user_wide: Vec<u16> = std::ffi::OsStr::new(user)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let cred = CREDENTIALW {
            flags: 0,
            cred_type: 1, // CRED_TYPE_GENERIC
            target_name: target_wide.as_ptr(),
            comment: ptr::null(),
            last_written: FILETIME {
                dw_low_date_time: 0,
                dw_high_date_time: 0,
            },
            credential_blob_size: secret.len() as u32,
            credential_blob: secret.as_ptr(),
            persist: 2, // CRED_PERSIST_LOCAL_MACHINE
            attribute_count: 0,
            attributes: ptr::null(),
            target_alias: ptr::null(),
            user_name: user_wide.as_ptr(),
        };

        unsafe {
            let _ = CredDeleteW(target_wide.as_ptr(), 1, 0);
            let res = CredWriteW(&cred, 0);
            if res == 0 {
                let err = std::io::Error::last_os_error();
                return Err(format!("CredWriteW failed: {}", err));
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn delete_credential(target: &str) -> Result<(), String> {
        let target_wide: Vec<u16> = std::ffi::OsStr::new(target)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            let _ = CredDeleteW(target_wide.as_ptr(), 1, 0);
        }
        Ok(())
    }
}

/// macOS Keychain. Antigravity's `go-keyring` turns the Windows target `gemini:antigravity` into
/// a generic password with service `gemini` and account `antigravity`.
#[cfg(target_os = "macos")]
mod system_cred {
    fn service(target: &str) -> &str {
        target.split_once(':').map_or(target, |(service, _)| service)
    }

    pub fn read_credential(target: &str) -> Result<Option<String>, String> {
        crate::keychain::read(service(target), super::TARGET_USERNAME)
    }

    pub fn write_credential(target: &str, user: &str, secret: &[u8]) -> Result<(), String> {
        let secret = std::str::from_utf8(secret).map_err(|error| error.to_string())?;
        crate::keychain::write(service(target), user, secret)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod system_cred {
    pub fn read_credential(_target: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    pub fn write_credential(_target: &str, _user: &str, _secret: &[u8]) -> Result<(), String> {
        Err(format!("switching accounts is not supported on this OS yet ({})", super::CREDENTIAL_STORE))
    }
}

/// Read the active Antigravity sign-in: the standalone app's token file (Antigravity 2.24+)
/// when present, otherwise the IDE's entry in OS credential store.
pub fn read_system_antigravity_credential() -> Result<Option<KeyringPayload>> {
    if let Some(payload) = read_standalone_token_file()? {
        return Ok(Some(payload));
    }
    let raw = system_cred::read_credential(TARGET_CREDENTIAL)
        .map_err(|e| HubError::Operation(format!("Credential read failed: {}", e)))?;

    let Some(content) = raw else {
        return Ok(None);
    };

    let payload: KeyringPayload = serde_json::from_str(&content)
        .map_err(|e| HubError::Operation(format!("Credential parsing failed: {}", e)))?;
    Ok(Some(payload))
}

/// Write an Antigravity sign-in where both flavours read it: the standalone app's token file
/// (only when it already exists, Antigravity 2.24-2.31) and the `gemini:antigravity` entry in
/// the OS credential store (IDE, and the standalone app from 2.32).
/// A failed IDE entry is only an error when the token file was not written either.
pub fn write_system_antigravity_credential(payload: &KeyringPayload) -> Result<()> {
    let wrote_file = write_standalone_token_file(payload)?;
    let mut json = serde_json::to_string(payload)
        .map_err(|e| HubError::Operation(format!("Credential serialization failed: {}", e)))?;
    if json.len() > CREDENTIAL_BLOB_LIMIT {
        // Windows refuses generic credentials over 2560 bytes ("not enough memory"); the
        // id_token is the optional bulk, and Antigravity fetches a new one on refresh.
        let mut slim = payload.clone();
        slim.token.id_token = None;
        json = serde_json::to_string(&slim)
            .map_err(|e| HubError::Operation(format!("Credential serialization failed: {}", e)))?;
    }

    let stored = system_cred::write_credential(TARGET_CREDENTIAL, TARGET_USERNAME, json.as_bytes())
        .map_err(|e| HubError::Operation(format!("Credential write failed: {}", e)));
    match stored {
        Err(error) if wrote_file => {
            tracing::warn!(%error, "Antigravity IDE credential not updated; the standalone token file was");
            Ok(())
        }
        other => other,
    }
}

/// Largest blob OS credential store accepts for a generic credential.
const CREDENTIAL_BLOB_LIMIT: usize = 2560;

/// `~/.gemini`, where the standalone Antigravity app (2.24+) keeps its data and sign-in.
fn gemini_home() -> Option<std::path::PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| std::path::PathBuf::from(home).join(".gemini"))
}

/// The standalone app's sign-in file. Its language server reads it at start, so a switch writes
/// it and restarts the server. Same fields as the IDE credential, except `id_token` sits at the
/// top level instead of inside `token`.
fn standalone_token_path() -> Option<std::path::PathBuf> {
    gemini_home().map(|dir| dir.join("jetski-standalone-oauth-token"))
}

fn read_standalone_token_file() -> Result<Option<KeyringPayload>> {
    let Some(path) = standalone_token_path().filter(|path| path.is_file()) else {
        return Ok(None);
    };
    let content = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| HubError::Operation(format!("Antigravity token file is not valid JSON: {e}")))?;
    Ok(Some(payload_from_standalone(&value)?))
}

fn payload_from_standalone(value: &serde_json::Value) -> Result<KeyringPayload> {
    let mut token: KeyringTokenDetails = serde_json::from_value(value["token"].clone())
        .map_err(|e| HubError::Operation(format!("Antigravity token file has an unexpected shape: {e}")))?;
    if token.id_token.is_none() {
        token.id_token = value["id_token"].as_str().map(str::to_owned);
    }
    Ok(KeyringPayload {
        token,
        auth_method: value["auth_method"].as_str().unwrap_or("consumer").to_owned(),
        ..Default::default()
    })
}

/// Updates the known fields of the token file and keeps any others the app may have added.
/// Writes through a temporary file so a crash never leaves a half-written sign-in behind.
/// Does nothing and returns `false` when the file does not exist: Antigravity 2.32+ moved the
/// sign-in into Credential Manager and can heal it from a leftover file, so one is never created.
fn write_standalone_token_file(payload: &KeyringPayload) -> Result<bool> {
    let Some(path) = standalone_token_path().filter(|path| path.is_file()) else {
        return Ok(false);
    };
    let existing = std::fs::read_to_string(&path).ok().and_then(|raw| serde_json::from_str(&raw).ok());
    let value = standalone_from_payload(existing, payload);
    let temp = path.with_extension("hub-tmp");
    write_private(&temp, &serde_json::to_vec(&value).map_err(|e| HubError::Operation(e.to_string()))?)?;
    std::fs::rename(&temp, &path)?;
    Ok(true)
}

/// Writes a file only the current user can read. The token file holds a Google refresh token, and
/// on macOS a plain write would leave it readable by every account on the computer (mode 0644).
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        // A leftover temp file keeps its old mode on open, so tighten it explicitly too.
        let mut file = options.open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        return file.write_all(bytes);
    }
    #[cfg(not(unix))]
    options.open(path)?.write_all(bytes)
}

fn standalone_from_payload(existing: Option<serde_json::Value>, payload: &KeyringPayload) -> serde_json::Value {
    let mut value = existing.filter(serde_json::Value::is_object).unwrap_or_else(|| serde_json::json!({}));
    let mut token = value["token"].as_object().cloned().unwrap_or_default();
    token.insert("access_token".into(), payload.token.access_token.clone().into());
    token.insert("token_type".into(), payload.token.token_type.clone().into());
    token.insert("refresh_token".into(), payload.token.refresh_token.clone().into());
    token.insert("expiry".into(), payload.token.expiry.clone().into());
    value["token"] = token.into();
    value["auth_method"] = payload.auth_method.clone().into();
    match &payload.token.id_token {
        Some(id_token) => value["id_token"] = id_token.clone().into(),
        None => {
            // A stale id_token would name the previous account.
            value.as_object_mut().map(|object| object.remove("id_token"));
        }
    }
    value
}

pub(crate) fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub(crate) fn base64_url_decode(input: &str) -> Option<Vec<u8>> {
    let mut s = input.replace('-', "+").replace('_', "/");
    while s.len() % 4 != 0 {
        s.push('=');
    }
    let table = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        if chunk.len() < 4 {
            return None;
        }
        let b0 = table(chunk[0])?;
        let b1 = table(chunk[1])?;
        out.push((b0 << 2) | (b1 >> 4));
        if chunk[2] != b'=' {
            let b2 = table(chunk[2])?;
            out.push(((b1 & 0xF) << 4) | (b2 >> 2));
            if chunk[3] != b'=' {
                let b3 = table(chunk[3])?;
                out.push(((b2 & 0x3) << 6) | b3);
            }
        }
    }
    Some(out)
}

/// Checks a per-account proxy address and returns it trimmed. Only `http`, `https`, `socks5` and
/// `socks5h` URLs with a host are accepted.
pub fn validate_proxy_url(url: &str) -> Result<String> {
    let url = url.trim();
    let supported = ["http://", "https://", "socks5://", "socks5h://"]
        .iter()
        .any(|scheme| url.strip_prefix(scheme).is_some_and(|rest| !rest.is_empty()));
    if !supported || reqwest::Proxy::all(url).is_err() {
        return Err(HubError::Configuration(
            "proxy must look like http://host:port or socks5://user:pass@host:port".into(),
        ));
    }
    Ok(url.to_owned())
}

/// Builds the HTTP client for Google calls made on behalf of one pool account. With `proxy` set,
/// all traffic goes through it and a broken proxy makes the request fail; it never falls back to
/// a direct connection, which would expose the address the proxy is there to hide.
pub fn http_client(proxy: Option<&str>, timeout: Duration) -> Result<Client> {
    let mut builder = Client::builder().timeout(timeout);
    if let Some(url) = proxy.filter(|url| !url.trim().is_empty()) {
        let proxy = reqwest::Proxy::all(url.trim())
            .map_err(|error| HubError::Configuration(format!("invalid account proxy: {error}")))?;
        builder = builder.proxy(proxy);
    }
    builder.build().map_err(|error| HubError::Operation(error.to_string()))
}

/// Asks Google which subscription tier an account is on (`loadCodeAssist`), preferring the paid
/// tier over the default one. `None` when Google does not answer or reports no tier.
pub fn fetch_google_tier(access_token: &str, proxy: Option<&str>) -> Option<String> {
    let body: serde_json::Value = http_client(proxy, Duration::from_secs(10))
        .ok()?
        .post("https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist")
        .bearer_auth(access_token)
        .header(reqwest::header::USER_AGENT, "antigravity/windows")
        .json(&serde_json::json!({ "metadata": { "ideType": "ANTIGRAVITY" } }))
        .send()
        .ok()
        .filter(|response| response.status().is_success())?
        .json()
        .ok()?;
    tier_from_load_response(&body)
}

fn tier_from_load_response(body: &serde_json::Value) -> Option<String> {
    ["paidTier", "currentTier"]
        .iter()
        .find_map(|key| body.pointer(&format!("/{key}/id")).and_then(|id| id.as_str()))
        .map(str::to_owned)
}

/// Closes Antigravity, both the standalone app and the separately installed Antigravity IDE:
/// politely (window close, unsaved work can prompt) or, with `force`, by ending each process
/// tree. Returns whether any Antigravity process was found.
/// Windows runs `taskkill` for `Antigravity.exe` and `Antigravity IDE.exe`; macOS sends SIGTERM
/// to the main binary of every `Antigravity*.app`, or with `force` SIGKILL to every process
/// inside them (helpers and language server included).
pub fn close_antigravity(force: bool) -> Result<bool> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut found = false;
        for image in ["Antigravity.exe", "Antigravity IDE.exe"] {
            let mut command = std::process::Command::new("taskkill");
            command.args(["/IM", image, "/T"]);
            if force {
                command.arg("/F");
            }
            command.creation_flags(0x0800_0000);
            found |= command.output()?.status.success();
        }
        Ok(found)
    }
    #[cfg(not(windows))]
    {
        let mut command = std::process::Command::new("pkill");
        if force {
            command.args(["-9", "-f", "Antigravity[^/]*\\.app/Contents/"]);
        } else {
            command.args(["-f", "Antigravity[^/]*\\.app/Contents/MacOS/"]);
        }
        Ok(command.output()?.status.success())
    }
}

/// Refreshes access token via Google OAuth token endpoint
pub fn refresh_google_oauth_token(refresh_token: &str, proxy: Option<&str>) -> Result<TokenRefreshResponse> {
    let client = http_client(proxy, Duration::from_secs(15))?;

    let client_id = get_oauth_client_id();
    let client_secret = get_oauth_client_secret();
    let body = format!(
        "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
        percent_encode(&client_id),
        percent_encode(&client_secret),
        percent_encode(refresh_token),
    );

    let resp = client
        .post(TOKEN_URL)
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .map_err(|e| HubError::Operation(format!("Token refresh request error: {}", e)))?;

    if !resp.status().is_success() {
        let err_text = resp.text().unwrap_or_default();
        return Err(HubError::Operation(format!(
            "Token refresh rejected by Google: {}",
            err_text
        )));
    }

    resp.json::<TokenRefreshResponse>()
        .map_err(|e| HubError::Operation(format!("Failed to parse token response: {}", e)))
}

/// How long a sign-in link stays valid for the localhost callback.
const OAUTH_STATE_LIFETIME: Duration = Duration::from_secs(15 * 60);
/// `state` values of sign-in links handed out recently. The callback only accepts a code that
/// comes back with one of them, so another program or web page cannot push its own Google
/// account into the pool by calling `localhost:8045` (OAuth login CSRF).
static ISSUED_OAUTH_STATES: Mutex<Vec<(String, std::time::Instant)>> = Mutex::new(Vec::new());

/// An unguessable hex string from the OS-seeded hasher keys (no extra randomness dependency).
fn random_hex() -> String {
    use std::hash::{BuildHasher, Hasher};
    (0..2)
        .map(|_| {
            let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
            hasher.write_u128(Utc::now().timestamp_nanos_opt().unwrap_or_default() as u128);
            format!("{:016x}", hasher.finish())
        })
        .collect()
}

fn issue_oauth_state() -> String {
    let state = random_hex();
    if let Ok(mut issued) = ISSUED_OAUTH_STATES.lock() {
        issued.retain(|(_, at)| at.elapsed() < OAUTH_STATE_LIFETIME);
        issued.push((state.clone(), std::time::Instant::now()));
    }
    state
}

/// Whether `state` belongs to a sign-in link this app handed out and is still valid. A state is
/// accepted once.
fn take_oauth_state(state: &str) -> bool {
    let Ok(mut issued) = ISSUED_OAUTH_STATES.lock() else { return false };
    issued.retain(|(_, at)| at.elapsed() < OAUTH_STATE_LIFETIME);
    match issued.iter().position(|(value, _)| value == state) {
        Some(index) => {
            issued.remove(index);
            true
        }
        None => false,
    }
}

/// Generate the browser authorization URL for Google OAuth
pub fn get_google_oauth_auth_url() -> String {
    format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=https://www.googleapis.com/auth/cloud-platform%20https://www.googleapis.com/auth/userinfo.email%20https://www.googleapis.com/auth/userinfo.profile&access_type=offline&prompt=consent&state={}",
        percent_encode(&get_oauth_client_id()),
        percent_encode(OAUTH_REDIRECT_URI),
        issue_oauth_state(),
    )
}

/// Exchange OAuth authorization code for tokens
pub fn exchange_google_oauth_code(code: &str) -> Result<TokenRefreshResponse> {
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| HubError::Operation(format!("HTTP client build failed: {}", e)))?;

    let client_id = get_oauth_client_id();
    let client_secret = get_oauth_client_secret();
    let body = format!(
        "code={}&client_id={}&client_secret={}&redirect_uri={}&grant_type=authorization_code",
        percent_encode(code),
        percent_encode(&client_id),
        percent_encode(&client_secret),
        percent_encode(OAUTH_REDIRECT_URI)
    );

    let resp = client
        .post(TOKEN_URL)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .map_err(|e| HubError::Operation(format!("OAuth code exchange network request failed: {}", e)))?;

    if !resp.status().is_success() {
        let err_text = resp.text().unwrap_or_default();
        return Err(HubError::Operation(format!(
            "Google OAuth authorization code exchange rejected: {}",
            err_text
        )));
    }

    resp.json::<TokenRefreshResponse>()
        .map_err(|e| HubError::Operation(format!("Failed to parse token response: {}", e)))
}

/// Listen on localhost:8045 for the OAuth browser callback and extract authorization code
pub fn listen_for_oauth_callback(timeout_secs: u64) -> Result<String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:8045")
        .map_err(|e| HubError::Operation(format!("Failed to bind port 8045 for OAuth callback: {}", e)))?;

    listener
        .set_nonblocking(true)
        .map_err(|e| HubError::Operation(format!("Failed to set TCP listener nonblocking: {}", e)))?;

    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_secs);

    loop {
        if start.elapsed() > timeout {
            return Err(HubError::Operation("OAuth login timed out waiting for browser callback (120s)".into()));
        }

        match listener.accept() {
            Ok((mut stream, _)) => {
                use std::io::Write;
                let request_line = read_request_line(&mut stream);
                let target = request_line.split_whitespace().nth(1).unwrap_or_default();
                let code = query_param(target, "code").filter(|code| !code.is_empty());
                // A code is only taken together with a `state` from a link this app issued.
                let code = code.filter(|_| query_param(target, "state").is_some_and(|state| take_oauth_state(&state)));
                // The user pressed "Cancel" on Google's consent screen: stop waiting right away.
                let denied = query_param(target, "error");

                let (status_line, body) = if code.is_some() {
                    (
                        "HTTP/1.1 200 OK",
                        callback_page(
                            "✦",
                            "Giriş Başarılı!",
                            "Hesabınız AI Usage Hub havuzuna eklendi.",
                            "Bu sekmeyi güvenle kapatabilirsiniz.",
                            "<script>setTimeout(function(){window.close();},2500);</script>",
                        ),
                    )
                } else {
                    (
                        "HTTP/1.1 400 Bad Request",
                        callback_page(
                            "!",
                            "Giriş Kodu Alınamadı",
                            "Google bir yetkilendirme kodu göndermedi.",
                            "AI Usage Hub'a dönüp girişi tekrar deneyin.",
                            "",
                        ),
                    )
                };

                let response = format!(
                    "{}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status_line,
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());

                if let Some(c) = code {
                    return Ok(c);
                }
                if let Some(error) = denied {
                    return Err(HubError::Operation(format!("Google sign-in was not completed: {error}")));
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(HubError::Operation(format!("TCP accept error: {}", e)));
            }
        }
    }
}

/// Reads the first line of an HTTP request from a freshly accepted connection. The socket is put
/// back into blocking mode with a short timeout first: on Windows it inherits the listener's
/// non-blocking mode, and a read that runs before the browser has sent anything (a preconnect)
/// would otherwise come back empty and lose the sign-in code.
fn read_request_line(stream: &mut std::net::TcpStream) -> String {
    use std::io::Read;
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 2048];
    while !buffer.contains(&b'\n') && buffer.len() < 16 * 1024 {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }
    }
    String::from_utf8_lossy(&buffer).lines().next().unwrap_or_default().to_owned()
}

/// Decodes `%XX` escapes and `+` (space) in a URL query value.
pub(crate) fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 3 <= bytes.len() => {
                match std::str::from_utf8(&bytes[index + 1..index + 3]).ok().and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        index += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            byte => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The decoded value of `key` in the query string of a request target or full URL.
fn query_param(target: &str, key: &str) -> Option<String> {
    let query = target.split_once('?')?.1;
    let query = query.split('#').next().unwrap_or(query);
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        (name == key).then(|| percent_decode(value))
    })
}

/// Accepts what a user pastes after signing in by hand: the bare code (as shown or still
/// URL-encoded), or the whole `http://localhost:8045/auth/callback?code=...` address.
pub fn extract_oauth_code(input: &str) -> Result<String> {
    let input = input.trim();
    let code = if input.contains("code=") {
        let target = if input.contains('?') { input.to_owned() } else { format!("?{input}") };
        query_param(&target, "code").unwrap_or_default()
    } else {
        percent_decode(input)
    };
    let code = code.trim().to_owned();
    if code.is_empty() {
        return Err(HubError::Configuration("paste the code (or the whole address) Google redirected to".into()));
    }
    Ok(code)
}

/// Fills the starlit page shown in the browser after the Google sign-in redirect.
/// All arguments are fixed strings from this file, so no HTML escaping is needed.
fn callback_page(icon: &str, title: &str, message: &str, hint: &str, script: &str) -> String {
    include_str!("oauth_callback.html")
        .replace("{{ICON}}", icon)
        .replace("{{TITLE}}", title)
        .replace("{{MESSAGE}}", message)
        .replace("{{HINT}}", hint)
        .replace("{{SCRIPT}}", script)
}

/// Export all Antigravity accounts from database as JSON string
pub fn export_antigravity_accounts_json(database: &Database) -> Result<String> {
    let accounts = database.antigravity_accounts()?;
    serde_json::to_string_pretty(&accounts)
        .map_err(|e| HubError::Operation(format!("Export JSON serialization failed: {}", e)))
}

fn lock(database: &Mutex<Database>) -> Result<MutexGuard<'_, Database>> {
    database
        .lock()
        .map_err(|_| HubError::Operation("database lock is unavailable".into()))
}

/// Pool id of the account signed in as `email`.
pub fn account_id(email: &str) -> String {
    format!("antigravity:{}", email.trim().to_lowercase())
}

/// Builds a pool account from a Google refresh token: renews the access token, asks Google whose
/// account it is and reads its quota. `email_hint` is only used when Google does not say; an
/// account whose email cannot be determined is refused rather than saved under a made-up name.
pub fn account_from_refresh_token(email_hint: &str, refresh_token: &str, proxy: Option<&str>) -> Result<AntigravityAccount> {
    let refresh_token = refresh_token.trim();
    if refresh_token.is_empty() {
        return Err(HubError::Configuration("the refresh token is empty".into()));
    }
    let token = refresh_google_oauth_token(refresh_token, proxy)?;
    let now = Utc::now().timestamp();
    let claims = token.id_token.as_deref().and_then(parse_jwt_claims);
    let (info_email, info_name) = fetch_google_account_info(&token.access_token);
    let email = info_email
        .or_else(|| claims.as_ref().map(|(email, _)| email.clone()))
        .or_else(|| Some(email_hint.trim().to_owned()).filter(|email| !email.is_empty()))
        .ok_or_else(|| HubError::Operation("Google did not report the account's email; enter it and try again".into()))?;
    let (quota_pct, quota_details) = fetch_google_quota_summary(&token.access_token, None, proxy)
        .map(|(quota, details)| (Some(quota), Some(details)))
        .unwrap_or((None, None));
    Ok(AntigravityAccount {
        id: account_id(&email),
        email,
        display_name: info_name.or_else(|| claims.and_then(|(_, name)| name)),
        access_token: token.access_token,
        refresh_token: token.refresh_token.unwrap_or_else(|| refresh_token.to_owned()),
        id_token: token.id_token,
        project_id: None,
        expiry_timestamp: now + token.expires_in,
        status: if quota_pct.is_some_and(|q| q <= 0.0) { "rate_limited".into() } else { "healthy".into() },
        is_current: false,
        last_used: now,
        quota_remaining_percent: quota_pct,
        quota_details,
        proxy_url: proxy.map(str::to_owned),
        tier: None,
    })
}

/// Imports pool accounts from JSON: either the hub's own export (full account objects) or a
/// minimal list of `{ "email": ..., "refresh_token": ... }` entries, as the import dialog
/// suggests. A single object is accepted as a one-entry list. Entries that cannot be imported are
/// skipped and logged; if none can be, the first problem is returned as the error.
pub fn import_antigravity_accounts_json(database: &Mutex<Database>, json_str: &str) -> Result<usize> {
    let value: serde_json::Value = serde_json::from_str(json_str.trim_start_matches('\u{feff}'))
        .map_err(|e| HubError::Operation(format!("Invalid import JSON format: {}", e)))?;
    let entries = match value {
        serde_json::Value::Array(entries) => entries,
        object @ serde_json::Value::Object(_) => vec![object],
        _ => return Err(HubError::Operation("Invalid import JSON format: expected a list of accounts".into())),
    };
    let mut count = 0;
    let mut first_error = None;
    for (index, entry) in entries.into_iter().enumerate() {
        match import_entry(&entry) {
            Ok(mut acc) => {
                let db = lock(database)?;
                db.merge_existing_antigravity_account(&mut acc)?;
                db.save_antigravity_account(&acc)?;
                count += 1;
            }
            Err(error) => {
                tracing::warn!(entry = index + 1, %error, "skipped an account during import");
                first_error.get_or_insert_with(|| HubError::Operation(format!("Import entry {}: {error}", index + 1)));
            }
        }
    }
    match (count, first_error) {
        (0, Some(error)) => Err(error),
        _ => Ok(count),
    }
}

fn import_entry(entry: &serde_json::Value) -> Result<AntigravityAccount> {
    if let Ok(mut acc) = serde_json::from_value::<AntigravityAccount>(entry.clone()) {
        if acc.email.trim().is_empty() {
            return Err(HubError::Operation("the account has no email".into()));
        }
        // Exports may come from another PC: whichever account is current there is not here.
        acc.is_current = false;
        if let Some((quota, details)) = fetch_google_quota_summary(&acc.access_token, acc.project_id.as_deref(), acc.proxy_url.as_deref()) {
            acc.quota_remaining_percent = Some(quota);
            acc.quota_details = Some(details);
        }
        return Ok(acc);
    }
    let text = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| entry.get(*key).and_then(serde_json::Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let refresh_token = text(&["refreshToken", "refresh_token"]).ok_or_else(|| HubError::Operation("no refresh token".into()))?;
    let proxy = text(&["proxyUrl", "proxy_url"]).map(|url| validate_proxy_url(&url)).transpose()?;
    account_from_refresh_token(&text(&["email"]).unwrap_or_default(), &refresh_token, proxy.as_deref())
}

/// Refreshes every pooled account that has a saved sign-in: renews tokens near expiry and pulls
/// fresh quota from Google. The database lock is taken only around each save, never across the
/// network calls, so the rest of the app stays responsive while Google is slow.
pub fn refresh_all_accounts(database: &Mutex<Database>) -> Result<Vec<AntigravityAccount>> {
    let accounts = lock(database)?.antigravity_accounts()?;
    let mut updated = Vec::new();
    let now = Utc::now().timestamp();

    for mut acc in accounts {
        if !crate::google_control::native_account_enabled(&lock(database)?.connection, &acc.id)
            .map_err(HubError::Operation)? {
            updated.push(acc);
            continue;
        }
        // Accounts seen only through the live session have no token to ask Google with.
        if acc.refresh_token.is_empty() {
            updated.push(acc);
            continue;
        }
        if acc.expiry_timestamp <= now + 60 {
            if let Ok(token_resp) = refresh_google_oauth_token(&acc.refresh_token, acc.proxy_url.as_deref()) {
                acc.access_token = token_resp.access_token;
                acc.expiry_timestamp = now + token_resp.expires_in;
                if let Some(refresh_token) = token_resp.refresh_token {
                    acc.refresh_token = refresh_token;
                }
                if let Some(id_tok) = token_resp.id_token {
                    acc.id_token = Some(id_tok);
                }
            }
        }

        if let Some(tier) = fetch_google_tier(&acc.access_token, acc.proxy_url.as_deref()) {
            acc.tier = Some(tier);
        }
        match fetch_google_quota_summary(&acc.access_token, acc.project_id.as_deref(), acc.proxy_url.as_deref()) {
            Some((quota, details)) => {
                acc.quota_remaining_percent = Some(quota);
                acc.quota_details = Some(details);
                // A readable quota with a working token also clears `forbidden` and `expired`:
                // the proxy flags `forbidden` on any 403, which can be a one-off (a model the
                // plan does not include) and used to bench the account for good. A real ban
                // fails this read, or is flagged again by the next 403.
                acc.status = if quota <= 0.0 { "rate_limited" } else { "healthy" }.into();
            }
            None => {
                heal_if_reset(&mut acc, now);
            }
        }

        // Not a plain save: the account may have been deleted while Google was answering.
        if lock(database)?.update_antigravity_account(&acc).unwrap_or(false) {
            updated.push(acc);
        }
    }

    Ok(updated)
}

/// Clears a `rate_limited` flag once the quota window that caused it has rolled over, so an
/// account does not stay benched forever when Google's quota endpoint cannot be reached.
/// Returns whether the account changed.
fn heal_if_reset(acc: &mut AntigravityAccount, now: i64) -> bool {
    if acc.status != "rate_limited" {
        return false;
    }
    let known_reset = acc
        .quota_details
        .as_ref()
        .and_then(|details| details.gemini_reset_at.max(details.claude_reset_at));
    // ponytail: without a reset time newer than the last use, assume the rolling 5-hour window;
    // a weekly-limit lockout heals too early and is re-flagged by the next 429 or quota read.
    let reset_at = match known_reset {
        Some(reset) if reset > acc.last_used => reset,
        _ => acc.last_used + 5 * 3600,
    };
    if now < reset_at {
        return false;
    }
    // Only the flag is cleared. The stored percentage stays as last measured until the next
    // real quota read, so a healed account is not advertised as full.
    acc.status = "healthy".into();
    true
}

fn vscdb_item(key: &str) -> Option<String> {
    let appdata = crate::platform::app_data_dir()?;
    ["Antigravity", "Antigravity IDE"].iter().find_map(|flavour| {
        let path = appdata.join(flavour).join("User/globalStorage/state.vscdb");
        if !path.exists() {
            return None;
        }
        // `?`, `#` and `%` would end or alter a SQLite URI path ("Application Support" has a space).
        let escaped = path
            .to_string_lossy()
            .replace('\\', "/")
            .replace('%', "%25")
            .replace(' ', "%20")
            .replace('?', "%3f")
            .replace('#', "%23");
        let uri = format!("file:{escaped}?mode=ro");
        rusqlite::Connection::open_with_flags(
            &uri,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )
        .ok()?
        .query_row("SELECT value FROM ItemTable WHERE key = ?1 LIMIT 1", [key], |row| row.get(0))
        .ok()
    })
}

fn varint(bytes: &mut &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let (&byte, rest) = bytes.split_first()?;
        *bytes = rest;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// Splits a protobuf message into `(field number, varint value, length-delimited payload)`.
/// Only the two wire types Antigravity's saved state uses are understood; parsing stops at
/// anything else.
fn proto_fields(mut bytes: &[u8]) -> Vec<(u64, u64, &[u8])> {
    let mut fields = Vec::new();
    while !bytes.is_empty() {
        let Some(tag) = varint(&mut bytes) else { break };
        match tag & 7 {
            0 => {
                let Some(value) = varint(&mut bytes) else { break };
                fields.push((tag >> 3, value, &bytes[..0]));
            }
            2 => {
                let Some(length) = varint(&mut bytes).map(|length| length as usize) else { break };
                if length > bytes.len() {
                    break;
                }
                fields.push((tag >> 3, 0, &bytes[..length]));
                bytes = &bytes[length..];
            }
            _ => break,
        }
    }
    fields
}

/// Extracts the Google One AI credit balance from the base64 `modelCredits` state value: a list of
/// `{key, {base64 message}}` entries where `availableCreditsSentinelKey` wraps the balance.
fn ai_credits_from_state(value: &str) -> Option<f64> {
    let outer = base64_url_decode(value.trim())?;
    proto_fields(&outer).into_iter().filter(|field| field.0 == 1).find_map(|(_, _, entry)| {
        let fields = proto_fields(entry);
        let key = fields.iter().find(|field| field.0 == 1)?.2;
        if key != b"availableCreditsSentinelKey" {
            return None;
        }
        let wrapper = fields.iter().find(|field| field.0 == 2)?.2;
        let encoded = proto_fields(wrapper).into_iter().find(|field| field.0 == 1)?.2;
        let inner = base64_url_decode(std::str::from_utf8(encoded).ok()?)?;
        proto_fields(&inner).into_iter().find(|field| field.0 == 2).map(|field| field.1 as f64)
    })
}

/// Google One AI credit balance as Antigravity last saved it in its own state database.
/// It belongs to whichever account the IDE was signed in with and is only as fresh as the IDE's
/// last write, so callers should prefer a balance reported by the language server.
pub fn read_vscdb_ai_credits() -> Option<f64> {
    ai_credits_from_state(&vscdb_item("antigravityUnifiedStateSync.modelCredits")?)
}

/// Reads saved account information from Antigravity's state.vscdb storage
pub fn read_vscdb_antigravity_account() -> Option<(String, Option<String>)> {
    let val: serde_json::Value = serde_json::from_str(&vscdb_item("antigravityAuthStatus")?).ok()?;
    let email = val.get("email")?.as_str().filter(|email| !email.is_empty())?.to_string();
    let name = val.get("name").and_then(|v| v.as_str()).map(str::to_string);
    Some((email, name))
}

/// Authoritative detection of the active Antigravity session across:
/// 1. Live Language Server process (actively running IDE)
/// 2. OS credential store (Windows Credential Manager / macOS Keychain, gemini:antigravity)
/// 3. VS Code / Antigravity state.vscdb
pub fn detect_active_antigravity_session() -> AntigravityLiveSession {
    let now = Utc::now().timestamp();

    // Priority 1: Live Language Server process (what IDE is actively communicating with)
    if let Some(live) = crate::providers::google::live_session() {
        if let Some(ref email) = live.email {
            let (quota_remaining_percent, details) = quota_details_from_usage(&live.usage);
            return AntigravityLiveSession {
                is_running: true,
                email: Some(email.clone()),
                name: live.name,
                plan: live.plan,
                source: format!("LanguageServer (Live Port: {})", live.port),
                port: Some(live.port),
                verified_at: now,
                quota_details: Some(details),
                quota_remaining_percent,
            };
        }
    }

    // Priority 2: OS credential store (when IDE is closed)
    if let Ok(Some(cred)) = read_system_antigravity_credential() {
        let (email_opt, name_opt) = payload_identity(&cred)
            .map(|(e, n)| (Some(e), n))
            .unwrap_or_else(|| fetch_google_account_info(&cred.token.access_token));

        if let Some(email) = email_opt {
            let (quota_pct, quota_details) = fetch_google_quota_summary(&cred.token.access_token, cred.token.project_id.as_deref(), None)
                .map(|(q, d)| (Some(q), Some(d)))
                .unwrap_or((None, None));

            return AntigravityLiveSession {
                is_running: false,
                email: Some(email),
                name: name_opt,
                plan: Some(format!("Antigravity ({CREDENTIAL_STORE})")),
                source: CREDENTIAL_STORE.into(),
                port: None,
                verified_at: now,
                quota_details,
                quota_remaining_percent: quota_pct,
            };
        }
    }

    // Priority 3: state.vscdb
    if let Some((email, name)) = read_vscdb_antigravity_account() {
        return AntigravityLiveSession {
            is_running: false,
            email: Some(email),
            name,
            plan: Some("Antigravity (Local State)".into()),
            source: "Antigravity Storage (state.vscdb)".into(),
            port: None,
            verified_at: now,
            quota_details: None,
            quota_remaining_percent: None,
        };
    }

    AntigravityLiveSession {
        is_running: false,
        email: None,
        name: None,
        plan: None,
        source: "None".into(),
        port: None,
        verified_at: now,
        quota_details: None,
        quota_remaining_percent: None,
    }
}

/// Synchronizes the active detected Antigravity session with the database pool.
/// Sets `is_current = true` on the verified account, `is_current = false` on others,
/// and auto-imports new accounts so they appear immediately. Detection talks to the
/// language server and Google, so the database lock is only taken afterwards.
pub fn sync_antigravity_active_session(database: &Mutex<Database>) -> Result<AntigravityLiveSession> {
    let session = detect_active_antigravity_session();
    let Some(ref email) = session.email else {
        return Ok(session);
    };
    let email_lower = email.to_lowercase();
    // The pool row may carry an id other than the usual one (an older import), so the id of the
    // row that matched by email is the one marked current.
    let mut target_id = account_id(email);
    let mut found = false;
    {
        let db = lock(database)?;
        for mut acc in db.antigravity_accounts()? {
            let mut changed = heal_if_reset(&mut acc, session.verified_at);
            if !found && acc.email.to_lowercase() == email_lower {
                found = true;
                target_id = acc.id.clone();
                changed = true;
                acc.is_current = true;
                acc.last_used = session.verified_at;
                if let Some(q) = session.quota_remaining_percent {
                    acc.quota_remaining_percent = Some(q);
                    if q > 0.0 && acc.status == "rate_limited" {
                        acc.status = "healthy".into();
                    }
                }
                if session.quota_details.is_some() {
                    acc.quota_details = session.quota_details.clone();
                }
            } else if acc.is_current {
                changed = true;
                acc.is_current = false;
            }
            if changed {
                let _ = db.save_antigravity_account(&acc);
            }
        }
    }

    if !found {
        // Auto-capture credentials from system if available
        let captured = capture_current_account(database).is_ok_and(|cap| cap.email.to_lowercase() == email_lower);
        if !captured {
            // No sign-in token for this account yet: it is listed, but cannot be switched to
            // until it is captured or signed in through the hub.
            let new_acc = AntigravityAccount {
                id: target_id.clone(),
                email: email.clone(),
                display_name: session.name.clone(),
                access_token: String::new(),
                refresh_token: String::new(),
                id_token: None,
                project_id: None,
                expiry_timestamp: 0,
                status: "healthy".into(),
                is_current: true,
                last_used: session.verified_at,
                quota_remaining_percent: session.quota_remaining_percent,
                quota_details: session.quota_details.clone(),
                proxy_url: None,
                tier: None,
            };
            let _ = lock(database)?.save_antigravity_account(&new_acc);
        }
    }

    let _ = lock(database)?.set_current_antigravity_account(&target_id);
    Ok(session)
}

/// Scans local credentials and live sessions to discover or update accounts
pub fn scan_local_accounts(database: &Mutex<Database>) -> Result<Vec<AntigravityAccount>> {
    // Capture first so the live sync that follows has the final say on which account is current.
    let _ = capture_current_account(database);
    let _ = sync_antigravity_active_session(database);
    lock(database)?.antigravity_accounts()
}

/// Parse email and name from ID Token (JWT without signature verification)
pub fn parse_jwt_claims(id_token: &str) -> Option<(String, Option<String>)> {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let payload_bytes = base64_url_decode(parts[1])?;
    let val: serde_json::Value = serde_json::from_slice(&payload_bytes).ok()?;
    let email = val.get("email")?.as_str()?.to_string();
    let name = val.get("name").and_then(|n| n.as_str()).map(str::to_string);
    Some((email, name))
}

#[derive(Debug, Deserialize)]
pub struct GoogleUserInfo {
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CloudCodeQuotaResponse {
    pub groups: Option<Vec<CloudCodeQuotaGroup>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudCodeQuotaGroup {
    pub display_name: Option<String>,
    pub buckets: Option<Vec<CloudCodeQuotaBucket>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudCodeQuotaBucket {
    pub bucket_id: Option<String>,
    pub display_name: Option<String>,
    pub remaining_fraction: Option<f64>,
    pub reset_time: Option<String>,
}

/// Fetch real Google account email and display name via Google OAuth UserInfo endpoint
pub fn fetch_google_account_info(access_token: &str) -> (Option<String>, Option<String>) {
    let client = match Client::builder().timeout(Duration::from_secs(8)).build() {
        Ok(c) => c,
        Err(_) => return (None, None),
    };

    let resp = client
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(access_token)
        .send();

    if let Ok(r) = resp {
        if r.status().is_success() {
            if let Ok(info) = r.json::<GoogleUserInfo>() {
                return (info.email, info.name);
            }
        }
    }
    (None, None)
}

/// Fetch raw CloudCode quota groups and buckets from Google
pub fn fetch_google_quota_response(
    access_token: &str,
    project_id: Option<&str>,
    proxy: Option<&str>,
) -> Option<CloudCodeQuotaResponse> {
    let client = http_client(proxy, Duration::from_secs(10)).ok()?;
    let project = project_id.unwrap_or("aicode-consumers");

    for url in &[
        "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
        "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
    ] {
        for body in &[
            serde_json::json!({ "project": project }),
            serde_json::json!({}),
        ] {
            let resp = client
                .post(*url)
                .bearer_auth(access_token)
                .header(reqwest::header::USER_AGENT, CLIENT_USER_AGENT)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .json(body)
                .send();

            if let Ok(r) = resp {
                if r.status().is_success() {
                    if let Ok(summary) = r.json::<CloudCodeQuotaResponse>() {
                        if summary.groups.as_ref().is_some_and(|g| !g.is_empty()) {
                            return Some(summary);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Maps quota rows onto the per-model breakdown the account pool shows and a representative
/// percentage: the mean of the rolling 5-hour windows, which is what limits immediate use.
/// The percentage is `None` when no 5-hour window is present rather than a made-up 100%.
pub fn quota_details_from_usage(usage: &[UsageValue]) -> (Option<f64>, AntigravityQuotaDetails) {
    let mut details = AntigravityQuotaDetails::default();
    for row in usage {
        let Some(value) = row.value else { continue };
        let metric = row.metric.to_ascii_lowercase();
        if metric.contains("argon") {
            // Gemini 4 Argon has its own weekly allowance (bucket `gemini-argon-wtu-weekly`).
            if metric.contains("weekly") {
                details.argon_weekly = Some(value);
                details.argon_weekly_reset_at = row.reset_at;
                details.argon_read_at = Some(row.retrieved_at);
            }
        } else if metric.contains("gemini-5h") || metric.contains("gemini_5h") {
            details.gemini_5h = Some(value);
            details.gemini_reset_at = row.reset_at;
        } else if metric.contains("gemini-weekly") || metric.contains("gemini_weekly") {
            details.gemini_weekly = Some(value);
            details.gemini_weekly_reset_at = row.reset_at;
        } else if metric.contains("3p-5h") || metric.contains("claude-5h") || metric.contains("3p_5h") {
            details.claude_5h = Some(value);
            details.claude_reset_at = row.reset_at;
        } else if metric.contains("3p-weekly") || metric.contains("claude-weekly") || metric.contains("3p_weekly") {
            details.claude_weekly = Some(value);
            details.claude_weekly_reset_at = row.reset_at;
        }
    }
    let representative = match (details.gemini_5h, details.claude_5h) {
        (Some(g), Some(c)) => Some((g + c) / 2.0),
        (g, c) => g.or(c),
    };
    (representative, details)
}

/// Fetch representative usable quota percentage (0-100) and multi-model breakdown
/// for an account straight from Google, without a running IDE.
pub fn fetch_google_quota_summary(
    access_token: &str,
    project_id: Option<&str>,
    proxy: Option<&str>,
) -> Option<(f64, AntigravityQuotaDetails)> {
    let (representative, details) = quota_details_from_usage(&fetch_google_usage_buckets(access_token, project_id, proxy));
    Some((representative?, details))
}

/// Convert CloudCode quota response to UsageValues
pub fn fetch_google_usage_buckets(access_token: &str, project_id: Option<&str>, proxy: Option<&str>) -> Vec<UsageValue> {
    fetch_google_quota_response(access_token, project_id, proxy)
        .map(|summary| usage_from_quota_response(summary, Utc::now().timestamp()))
        .unwrap_or_default()
}

fn usage_from_quota_response(summary: CloudCodeQuotaResponse, now: i64) -> Vec<UsageValue> {
    let mut values = Vec::new();
    for group in summary.groups.unwrap_or_default() {
        let group_name = group.display_name.unwrap_or_else(|| "Models".into());
        for bucket in group.buckets.unwrap_or_default() {
            let bucket_id = bucket.bucket_id.unwrap_or_else(|| "model".into());
            let display_name = bucket.display_name.unwrap_or_else(|| bucket_id.clone());
            let reset_at = bucket
                .reset_time
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.timestamp());

            let mut frac = bucket.remaining_fraction.unwrap_or(1.0);
            if reset_at.is_some_and(|reset_ts| now >= reset_ts) {
                frac = 1.0; // Reset time has passed; quota restored
            }

            values.push(UsageValue {
                metric: format!("antigravity_{}_remaining", bucket_id),
                label: format!("{} · {}", group_name, display_name),
                value: Some((frac * 100.0).clamp(0.0, 100.0)),
                max_value: Some(100.0),
                unit: "% remaining".into(),
                source: UsageSource::OfficialApi,
                confidence: Confidence::High,
                retrieved_at: now,
                reset_at,
            });
        }
    }
    values
}

/// Hot-switch: terminates language_server processes so Antigravity IDE restarts them
/// with the new account without closing the editor window.
pub fn kill_antigravity_language_servers() -> usize {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_exe(sysinfo::UpdateKind::OnlyIfNotSet)
            .with_cmd(sysinfo::UpdateKind::Always),
    );

    let mut killed = 0;
    for process in system.processes().values() {
        let name_lower = process.name().to_string_lossy().to_ascii_lowercase();
        let executable = process
            .exe()
            .map(|path| path.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        // language_server_windows_x64.exe, language_server_macos_arm, ...
        let is_ls = name_lower.starts_with("language_server");
        let is_antigravity = executable.contains("antigravity")
            || process.cmd().iter().any(|c| c.to_string_lossy().to_ascii_lowercase().contains("antigravity"));

        if is_ls && is_antigravity {
            if process.kill() {
                killed += 1;
            }
        }
    }
    killed
}

/// Capture current account from the OS credential store, UserInfo, and Google CloudCode APIs
pub fn capture_current_account(database: &Mutex<Database>) -> Result<AntigravityAccount> {
    let cred = read_system_antigravity_credential()?
        .ok_or_else(|| HubError::Operation("No Antigravity sign-in found on this computer. Please sign in to Antigravity first.".into()))?;

    let now = Utc::now().timestamp();
    let mut details = cred.token.clone();

    let expiry_ts = DateTime::parse_from_rfc3339(&details.expiry)
        .map(|dt| dt.timestamp())
        .unwrap_or(now + 3600);

    let mut access_token = details.access_token.clone();

    // Check if token is expired or expiring within 60s
    if now >= expiry_ts - 60 {
        if let Ok(token_resp) = refresh_google_oauth_token(&details.refresh_token, None) {
            access_token = token_resp.access_token.clone();
            details.access_token = token_resp.access_token;
            if let Some(ref rt) = token_resp.refresh_token {
                details.refresh_token = rt.clone();
            }
            if let Some(ref it) = token_resp.id_token {
                details.id_token = Some(it.clone());
            }
            let _ = write_system_antigravity_credential(&KeyringPayload {
                token: details.clone(),
                ..cred.clone()
            });
        }
    }

    // 1. Try to get email and display name from the stored sign-in (ID token or id_token_email)
    let (mut email_opt, mut display_name) = payload_identity(&KeyringPayload { token: details.clone(), ..cred })
        .map(|(e, n)| (Some(e), n))
        .unwrap_or((None, None));

    // 2. If email is missing, fetch from Google OAuth UserInfo API directly!
    if email_opt.is_none() {
        let (u_email, u_name) = fetch_google_account_info(&access_token);
        if let Some(e) = u_email {
            email_opt = Some(e);
        }
        if display_name.is_none() {
            display_name = u_name;
        }
    }

    let email = email_opt.ok_or_else(|| {
        HubError::Operation("Could not tell which Google account Antigravity is signed in with; open Antigravity and try again.".into())
    })?;

    // 3. Fetch real quota percentage directly from Google CloudCode internal quota API!
    let (mut quota_pct, mut quota_details) = match fetch_google_quota_summary(&access_token, details.project_id.as_deref(), None) {
        Some((pct, d)) => (Some(pct), Some(d)),
        None => (None, None),
    };

    // Fallback to the running IDE, but only when it is signed in as this same account;
    // otherwise its numbers belong to someone else.
    if quota_pct.is_none() {
        if let Some(live) = crate::providers::google::live_session() {
            if live.email.as_deref().is_some_and(|live_email| live_email.eq_ignore_ascii_case(&email)) {
                let (pct, d) = quota_details_from_usage(&live.usage);
                quota_pct = pct;
                quota_details = Some(d);
            }
        }
    }

    let id = format!("antigravity:{}", email.to_lowercase());
    let account = AntigravityAccount {
        id: id.clone(),
        email,
        display_name,
        access_token: details.access_token,
        refresh_token: details.refresh_token,
        id_token: details.id_token,
        project_id: details.project_id,
        expiry_timestamp: expiry_ts,
        status: if quota_pct.is_some_and(|q| q <= 0.0) { "rate_limited".into() } else { "healthy".into() },
        is_current: true,
        last_used: now,
        quota_remaining_percent: quota_pct,
        quota_details,
        proxy_url: None,
        tier: None,
    };

    let db = lock(database)?;
    db.save_antigravity_account(&account)?;
    db.set_current_antigravity_account(&id)?;
    drop(db);
    Ok(account)
}

/// Polls the language server until it reports a signed-in account again after a restart.
fn wait_for_live_email(timeout: Duration) -> Option<String> {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        std::thread::sleep(Duration::from_secs(1));
        if let Some(email) = crate::providers::google::live_account_email() {
            return Some(email);
        }
    }
    None
}

/// Switches Antigravity to a pooled account: writes its token to the OS credential store and
/// restarts the language server so the IDE picks it up.
///
/// Refuses accounts without a saved refresh token (writing an empty credential would sign the
/// IDE out). When the IDE was running, waits for the language server to come back and checks
/// which account it reports; if it is still the old one, the previous credential is restored
/// and an error is returned. Can block for up to ~15 s; the database lock is not held meanwhile.
pub fn switch_to_account(database: &Mutex<Database>, account_id: &str) -> Result<AntigravityAccount> {
    tracing::info!(account_id, "switch: requested");
    if !crate::google_control::native_account_enabled(&lock(database)?.connection, account_id)
        .map_err(HubError::Operation)? {
        return Err(HubError::Operation("Enable this account in the Google control center before switching.".into()));
    }
    let mut account = lock(database)?
        .antigravity_account(account_id)?
        .ok_or_else(|| HubError::Operation(format!("Account {} not found", account_id)))?;

    if account.refresh_token.is_empty() {
        return Err(HubError::Operation(format!(
            "{} has no saved sign-in token. Use Sign in with Google, or capture it while it is the active Antigravity account.",
            account.email
        )));
    }

    let now = Utc::now().timestamp();

    // Check if token is expired or expiring within 5 minutes
    if account.expiry_timestamp <= now + 300 {
        match refresh_google_oauth_token(&account.refresh_token, account.proxy_url.as_deref()) {
            Ok(refreshed) => {
                account.access_token = refreshed.access_token;
                account.expiry_timestamp = now + refreshed.expires_in;
                if let Some(new_rf) = refreshed.refresh_token {
                    account.refresh_token = new_rf;
                }
                if let Some(id_tok) = refreshed.id_token {
                    account.id_token = Some(id_tok);
                }
                account.status = "healthy".into();
            }
            Err(e) => {
                account.status = "expired".into();
                let _ = lock(database)?.update_antigravity_account(&account);
                return Err(HubError::Operation(format!(
                    "Could not refresh token for {}: {}",
                    account.email, e
                )));
            }
        }
    }

    let expiry_str = DateTime::from_timestamp(account.expiry_timestamp, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true);

    let previous = read_system_antigravity_credential().ok().flatten();
    let payload = KeyringPayload {
        token: KeyringTokenDetails {
            access_token: account.access_token.clone(),
            token_type: "Bearer".into(),
            refresh_token: account.refresh_token.clone(),
            expiry: expiry_str,
            id_token: account.id_token.clone(),
            project_id: account.project_id.clone(),
        },
        auth_method: previous
            .as_ref()
            .map_or_else(|| "consumer".into(), |credential| credential.auth_method.clone()),
        id_token_email: Some(account.email.clone()),
        extra: carried_fields(previous.as_ref(), &account.email),
    };

    // 1. Write the sign-in where Antigravity reads it
    let started = std::time::Instant::now();
    write_system_antigravity_credential(&payload)?;
    tracing::info!(account = %account.email, "switch: sign-in written");

    // 2. Hot switch: kill running language server so Antigravity picks up new account
    let killed = kill_antigravity_language_servers();
    tracing::info!(killed, "switch: language server stopped");

    // 3. Verify the IDE really came back as the new account
    let mut outcome = "Antigravity was not running; the account applies on its next start.".to_string();
    if killed > 0 {
        let live = wait_for_live_email(Duration::from_secs(15));
        tracing::info!(live = ?live, elapsed_ms = started.elapsed().as_millis() as u64, "switch: language server answered");
        match live {
            Some(live_email) if !live_email.eq_ignore_ascii_case(&account.email) => {
                if let Some(previous) = &previous {
                    let _ = write_system_antigravity_credential(previous);
                }
                return Err(HubError::Operation(format!(
                    "Antigravity is still signed in as {live_email}, so the switch to {} did not take effect and the previous credential was restored. Sign out inside Antigravity, then switch again.",
                    account.email
                )));
            }
            Some(_) => outcome = format!("Language server restarted ({killed} process(es)) and reports the new account."),
            None => outcome = "The language server did not come back within 15 seconds; restart Antigravity to finish the switch.".into(),
        }
    }

    // 4. Update database state and refresh quota
    account.is_current = true;
    account.last_used = now;
    if let Some((q, details)) = fetch_google_quota_summary(&account.access_token, account.project_id.as_deref(), account.proxy_url.as_deref()) {
        account.quota_remaining_percent = Some(q);
        account.quota_details = Some(details);
    }
    tracing::info!(elapsed_ms = started.elapsed().as_millis() as u64, "switch: quota fetched, waiting for database");
    let db = lock(database)?;
    if !db.update_antigravity_account(&account)? {
        return Err(HubError::Operation(format!(
            "{} was removed from the pool while switching; it was not added back.",
            account.email
        )));
    }
    db.set_current_antigravity_account(&account.id)?;

    // 5. Record activity event
    db.record_activity_event(
        "antigravity_switch",
        None, // `account_id` references `accounts`; pool ids live in `antigravity_accounts`.
        &format!("Switched Antigravity to {}", account.email),
        Some(&outcome),
        Severity::Success,
        now,
    )?;
    drop(db);
    tracing::info!(elapsed_ms = started.elapsed().as_millis() as u64, "switch: finished");

    Ok(account)
}

/// Calculates health score (0-100) for an Antigravity account based on quota percentage, status, and rest duration
pub fn calculate_health_score(quota_pct: Option<f64>, status: &str, last_used: i64) -> f64 {
    if status == "rate_limited" || status == "expired" || status == "forbidden" {
        return 0.0;
    }
    let base_quota = quota_pct.unwrap_or(50.0);
    let now = Utc::now().timestamp();
    let hours_rested = ((now - last_used).max(0) as f64 / 3600.0).min(24.0);
    let rest_bonus = (hours_rested / 24.0) * 15.0;
    (base_quota * 0.85 + rest_bonus).clamp(0.0, 100.0)
}

fn auto_switch_pool(database: &Database) -> Result<(Vec<AntigravityAccount>, std::collections::HashSet<String>)> {
    let accounts = database.antigravity_accounts()?;
    let mut enabled = std::collections::HashSet::new();
    for account in &accounts {
        if crate::google_control::native_account_enabled(&database.connection, &account.id)
            .map_err(HubError::Operation)? {
            enabled.insert(account.id.clone());
        }
    }
    Ok((accounts, enabled))
}
/// Minimum time between two automatic switch attempts, successful or not.
const AUTO_SWITCH_COOLDOWN: Duration = Duration::from_secs(180);
static LAST_AUTO_SWITCH: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// Evaluates if an auto-switch is needed and triggers it
pub fn check_and_auto_switch(state: &HubState) -> Result<Option<AntigravityAccount>> {
    let settings = state
        .database
        .lock()
        .map_err(|_| HubError::Operation("database lock unavailable".into()))?
        .settings()?;

    if !settings.antigravity_auto_switch {
        return Ok(None);
    }

    let (accounts, enabled_ids) = {
        let database = state.database.lock()
            .map_err(|_| HubError::Operation("database lock unavailable".into()))?;
        auto_switch_pool(&database)?
    };

    if accounts.len() < 2 {
        return Ok(None); // Need at least 2 accounts to switch
    }

    let current = accounts.iter().find(|a| a.is_current);

    // Lowest rolling 5-hour window of the current account, from the provider refresh the monitor
    // already ran. A reading that belongs to another account (e.g. the one just switched away
    // from) is ignored until the next refresh, so one depleted reading cannot trigger twice.
    let current_key = current.map(|c| format!("email:{}", c.email.to_lowercase()));
    let live_min_quota = state
        .providers
        .read()
        .map_err(|_| HubError::Operation("provider lock is unavailable".into()))?
        .iter()
        .find(|provider| provider.id == "google" && provider.error.is_none() && provider.account_key.is_some() && provider.account_key == current_key)
        .and_then(|provider| {
            provider
                .usage
                .iter()
                .filter(|u| u.unit == "% remaining" && u.metric.contains("5h"))
                .filter_map(|u| u.value)
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        });

    let threshold = settings.antigravity_switch_threshold as f64;
    let is_depleted = live_min_quota.is_some_and(|q| q <= threshold);
    let is_rate_limited = current.is_some_and(|c| c.status == "rate_limited");
    let is_forbidden = current.is_some_and(|c| c.status == "forbidden");

    if !is_depleted && !is_rate_limited && !is_forbidden {
        return Ok(None);
    }

    let current_email = current.map(|c| c.email.clone()).unwrap_or_default();
    tracing::info!(
        email = %current_email,
        min_quota = ?live_min_quota,
        threshold = threshold,
        "Antigravity account depleted or rate-limited; evaluating candidates for auto-switch"
    );

    // Candidates: another account that can actually be switched to and is not itself at or
    // below the threshold (otherwise two low accounts would bounce the IDE back and forth).
    let mut candidates: Vec<&AntigravityAccount> = accounts
        .iter()
        .filter(|a| {
            !a.is_current
                && enabled_ids.contains(&a.id)
                && !a.refresh_token.is_empty()
                && !matches!(a.status.as_str(), "expired" | "rate_limited" | "forbidden")
                && a.quota_remaining_percent.is_none_or(|q| q > threshold)
        })
        .collect();

    if candidates.is_empty() {
        tracing::warn!("Auto-switch: no healthy candidate accounts available in pool");
        return Ok(None);
    }

    // Sort by strategy: oldest last_used (round-robin), best tier first, or highest health score
    if settings.antigravity_switch_strategy == "round_robin" {
        candidates.sort_by_key(|a| a.last_used);
    } else if settings.antigravity_switch_strategy == "tier_priority" {
        candidates.sort_by(|a, b| {
            crate::routing::tier_rank(a.tier.as_deref())
                .cmp(&crate::routing::tier_rank(b.tier.as_deref()))
                .then(b.quota_remaining_percent.unwrap_or(100.0).total_cmp(&a.quota_remaining_percent.unwrap_or(100.0)))
        });
    } else {
        candidates.sort_by(|a, b| {
            let score_a = calculate_health_score(a.quota_remaining_percent, &a.status, a.last_used);
            let score_b = calculate_health_score(b.quota_remaining_percent, &b.status, b.last_used);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // The monitor calls this every few seconds. Without a pause after an attempt, a switch that
    // keeps failing (the IDE stays on the old account, the token is refused) would rewrite the
    // sign-in and restart Antigravity's language server over and over.
    {
        let mut last_attempt = LAST_AUTO_SWITCH.lock().map_err(|_| HubError::Operation("auto-switch lock is unavailable".into()))?;
        if last_attempt.is_some_and(|at| at.elapsed() < AUTO_SWITCH_COOLDOWN) {
            return Ok(None);
        }
        *last_attempt = Some(std::time::Instant::now());
    }

    let chosen = candidates[0].clone();
    let switched = switch_to_account(&state.database, &chosen.id)?;
    let db = lock(&state.database)?;

    let now = Utc::now().timestamp();
    db.record_activity_event(
        "antigravity_auto_switch",
        None, // `account_id` references `accounts`; pool ids live in `antigravity_accounts`.
        &format!("Auto-switched Antigravity to {}", switched.email),
        Some(&format!(
            "Quota fell below {}% (measured: {:?}%). Automatically rotated from {}.",
            threshold, live_min_quota, current_email
        )),
        Severity::Warning,
        now,
    )?;

    Ok(Some(switched))
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn token_file_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        super::write_private(&path, b"{\"token\":{}}").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"token\":{}}");
    }

    use super::*;

    #[test]
    fn standalone_token_file_round_trips_and_keeps_unknown_fields() {
        let existing = serde_json::json!({
            "token": { "access_token": "old", "token_type": "Bearer", "refresh_token": "old-r", "expiry": "x", "extra": 1 },
            "auth_method": "consumer", "id_token": "old-id", "future_field": true
        });
        let payload = KeyringPayload {
            token: KeyringTokenDetails {
                access_token: "new".into(), token_type: "Bearer".into(), refresh_token: "new-r".into(),
                expiry: "2026-10-17T00:00:00Z".into(), id_token: Some("new-id".into()), project_id: None,
            },
            auth_method: "consumer".into(),
            ..Default::default()
        };
        let written = standalone_from_payload(Some(existing), &payload);
        assert_eq!(written["id_token"], "new-id");
        assert_eq!(written["token"]["refresh_token"], "new-r");
        assert_eq!(written["token"]["extra"], 1);
        assert_eq!(written["future_field"], true);
        assert!(written["token"].get("id_token").is_none(), "id_token belongs at the top level");
        let read_back = payload_from_standalone(&written).unwrap();
        assert_eq!(read_back.token.access_token, "new");
        assert_eq!(read_back.token.id_token.as_deref(), Some("new-id"));

        let without_id = KeyringPayload { token: KeyringTokenDetails { id_token: None, ..payload.token.clone() }, ..payload };
        assert!(standalone_from_payload(Some(written), &without_id).get("id_token").is_none());
    }

    #[test]
    fn antigravity_232_credential_keeps_its_fields_and_names_the_new_account() {
        // Shape Antigravity 2.32 writes to Credential Manager: no id_token, email beside the token.
        let stored = r#"{"token":{"access_token":"a","token_type":"Bearer","refresh_token":"r","expiry":"2026-11-15T03:39:34Z","expires_in":3599},
            "auth_method":"consumer","id_token_email":"old@example.com","writer_id":"w1","user_tier":"g1-pro-tier"}"#;
        let previous: KeyringPayload = serde_json::from_str(stored).unwrap();
        assert_eq!(payload_identity(&previous), Some(("old@example.com".into(), None)));

        let same = carried_fields(Some(&previous), "OLD@example.com");
        assert_eq!(same.get("user_tier").and_then(|v| v.as_str()), Some("g1-pro-tier"));

        let switched = KeyringPayload {
            id_token_email: Some("new@example.com".into()),
            extra: carried_fields(Some(&previous), "new@example.com"),
            ..previous.clone()
        };
        let written: serde_json::Value = serde_json::to_value(&switched).unwrap();
        assert_eq!(written["writer_id"], "w1", "installation fields survive a switch");
        assert_eq!(written["id_token_email"], "new@example.com");
        assert!(written.get("user_tier").is_none(), "the old account's plan must not follow the new token");
    }

    #[test]
    fn argon_bucket_fills_its_own_fields_and_survives_a_google_refresh() {
        let row = |metric: &str, value: f64, reset: i64| UsageValue {
            metric: metric.into(), label: String::new(), value: Some(value), max_value: Some(100.0),
            unit: "% remaining".into(), source: UsageSource::LocalApplication, confidence: Confidence::High,
            retrieved_at: 50, reset_at: Some(reset),
        };
        let (_, live) = quota_details_from_usage(&[
            row("antigravity_gemini-weekly_remaining", 80.0, 1),
            row("antigravity_gemini-argon-wtu-weekly_remaining", 0.0, 99),
        ]);
        assert_eq!(live.gemini_weekly, Some(80.0), "the Argon bucket must not overwrite Gemini weekly");
        assert_eq!((live.argon_weekly, live.argon_weekly_reset_at, live.argon_read_at), (Some(0.0), Some(99), Some(50)));

        let (_, mut from_google) = quota_details_from_usage(&[row("antigravity_gemini-5h_remaining", 100.0, 2)]);
        from_google.keep_argon_from(Some(&live));
        assert_eq!(from_google.argon_weekly, Some(0.0));
        assert_eq!(from_google.gemini_5h, Some(100.0));
    }

    #[test]
    fn callback_page_fills_every_placeholder() {
        let page = callback_page("✦", "Başlık", "Mesaj", "İpucu", "");
        assert!(page.contains("<h1>Başlık</h1>") && page.contains("İpucu"));
        assert!(!page.contains("{{"), "unfilled placeholder left in the callback page");
    }

    #[test]
    fn proxy_urls_are_validated_and_tier_prefers_the_paid_plan() {
        assert_eq!(validate_proxy_url(" socks5://user:pw@10.0.0.2:1080 ").unwrap(), "socks5://user:pw@10.0.0.2:1080");
        assert!(validate_proxy_url("ftp://host:21").is_err());
        assert!(validate_proxy_url("http://").is_err());
        assert!(http_client(Some("http://127.0.0.1:9"), Duration::from_secs(1)).is_ok());

        let body = serde_json::json!({"currentTier": {"id": "free-tier"}, "paidTier": {"id": "g1-ultra-tier"}});
        assert_eq!(tier_from_load_response(&body).as_deref(), Some("g1-ultra-tier"));
        assert_eq!(tier_from_load_response(&serde_json::json!({})), None);
    }

    #[test]
    fn oauth_codes_are_decoded_from_any_pasted_form() {
        // Google may send the slash escaped; encoding it again on exchange broke the sign-in.
        assert_eq!(query_param("/auth/callback?code=4%2F0AbC-d&scope=x", "code").as_deref(), Some("4/0AbC-d"));
        assert_eq!(query_param("/auth/callback?error=access_denied", "error").as_deref(), Some("access_denied"));
        assert_eq!(query_param("/favicon.ico", "code"), None);
        assert_eq!(extract_oauth_code(" 4/0AbC-d ").unwrap(), "4/0AbC-d");
        assert_eq!(extract_oauth_code("4%2F0AbC-d").unwrap(), "4/0AbC-d");
        assert_eq!(extract_oauth_code("http://localhost:8045/auth/callback?code=4%2F0AbC-d&scope=email%20profile").unwrap(), "4/0AbC-d");
        assert!(extract_oauth_code("   ").is_err());
        assert_eq!(percent_decode("a%zz%2"), "a%zz%2", "malformed escapes are kept as typed");
    }

    #[test]
    fn sign_in_callbacks_need_a_state_this_app_issued_and_only_once() {
        let url = get_google_oauth_auth_url();
        let state = query_param(&url, "state").unwrap();
        assert_eq!(state.len(), 32);
        assert!(!take_oauth_state("forged"));
        assert!(take_oauth_state(&state));
        assert!(!take_oauth_state(&state), "a state is single-use");
    }

    #[test]
    fn minimal_import_entries_without_a_refresh_token_are_refused() {
        let directory = tempfile::tempdir().unwrap();
        let database = Mutex::new(Database::open(&directory.path().join("hub.sqlite3")).unwrap());
        let error = import_antigravity_accounts_json(&database, r#"[{"email":"a@example.com"}]"#).unwrap_err();
        assert!(error.to_string().contains("no refresh token"), "{error}");
        assert!(import_antigravity_accounts_json(&database, "42").is_err());
        assert_eq!(import_antigravity_accounts_json(&database, "[]").unwrap(), 0);
    }

    #[test]
    fn percent_encode_handles_special_characters() {
        assert_eq!(percent_encode("abc123-_.~"), "abc123-_.~");
        assert_eq!(percent_encode("foo bar+baz/1"), "foo%20bar%2Bbaz%2F1");
    }

    #[test]
    fn base64_url_decode_handles_unpadded_strings() {
        let decoded = base64_url_decode("aGVsbG8").expect("should decode hello");
        assert_eq!(String::from_utf8(decoded).unwrap(), "hello");
    }

    #[test]
    fn jwt_claims_parser_extracts_email_and_name() {
        // payload: {"email": "test@example.com", "name": "Test User"} -> base64url: eyJlbWFpbCI6ICJ0ZXN0QGV4YW1wbGUuY29tIiwgIm5hbWUiOiAiVGVzdCBVc2VyIn0
        let fake_jwt = "header.eyJlbWFpbCI6ICJ0ZXN0QGV4YW1wbGUuY29tIiwgIm5hbWUiOiAiVGVzdCBVc2VyIn0.signature";
        let claims = parse_jwt_claims(fake_jwt).expect("should parse claims");
        assert_eq!(claims.0, "test@example.com");
        assert_eq!(claims.1, Some("Test User".to_string()));
    }

    #[test]
    fn quota_response_maps_to_rows_and_pool_breakdown() {
        let summary: CloudCodeQuotaResponse = serde_json::from_value(serde_json::json!({
            "groups": [
                { "displayName": "Gemini Models", "buckets": [
                    { "bucketId": "gemini-weekly", "remainingFraction": 0.5, "resetTime": "2030-01-05T00:00:00Z" },
                    { "bucketId": "gemini-5h", "remainingFraction": 0.25, "resetTime": "2030-01-01T00:00:00Z" },
                ] },
                { "displayName": "Claude and GPT models", "buckets": [
                    { "bucketId": "3p-weekly", "remainingFraction": 0.05 },
                    { "bucketId": "3p-5h", "remainingFraction": 0.0, "resetTime": "2020-01-01T00:00:00Z" },
                ] },
            ]
        }))
        .unwrap();
        let rows = usage_from_quota_response(summary, 1_800_000_000);
        // Only what Google reported: no invented image-generation row.
        assert_eq!(rows.len(), 4);
        let (representative, details) = quota_details_from_usage(&rows);
        assert_eq!(details.gemini_5h, Some(25.0));
        assert_eq!(details.gemini_weekly, Some(50.0));
        assert_eq!(details.gemini_weekly_reset_at, Some(1_893_801_600));
        assert_eq!(details.claude_weekly_reset_at, None);
        assert_eq!(details.claude_weekly, Some(5.0));
        // The Claude 5-hour window reset in the past, so it counts as renewed.
        assert_eq!(details.claude_5h, Some(100.0));
        assert_eq!(representative, Some(62.5));
        assert_eq!(quota_details_from_usage(&[]).0, None);
    }

    #[test]
    fn ai_credit_balance_is_read_from_antigravity_state() {
        // Same layout as a real `modelCredits` value: threshold 50, credits enabled, balance 1000.
        let state = "CigKHm1pbmltdW1DcmVkaXRBbW91bnRGb3JVc2FnZUtleRIGCgRFREk9CiEKF3VzZUFJQ3JlZGl0c1NlbnRpbmVsS2V5EgYKBENBRT0KJQobYXZhaWxhYmxlQ3JlZGl0c1NlbnRpbmVsS2V5EgYKBEVPZ0g=";
        assert_eq!(ai_credits_from_state(state), Some(1000.0));
        assert_eq!(ai_credits_from_state("not base64 !"), None);
        assert_eq!(ai_credits_from_state(""), None);
    }

    fn pooled(status: &str, last_used: i64, reset_at: Option<i64>) -> AntigravityAccount {
        AntigravityAccount {
            id: "antigravity:a@example.com".into(),
            email: "a@example.com".into(),
            display_name: None,
            access_token: String::new(),
            refresh_token: String::new(),
            id_token: None,
            project_id: None,
            expiry_timestamp: 0,
            status: status.into(),
            is_current: false,
            last_used,
            quota_remaining_percent: Some(0.0),
            quota_details: reset_at.map(|reset| AntigravityQuotaDetails { gemini_reset_at: Some(reset), ..Default::default() }),
            proxy_url: None,
            tier: None,
        }
    }

    #[test]
    fn rate_limit_heals_only_after_the_window_resets() {
        let mut waiting = pooled("rate_limited", 1_000, Some(5_000));
        assert!(!heal_if_reset(&mut waiting, 4_999));
        assert!(heal_if_reset(&mut waiting, 5_000));
        assert_eq!(waiting.status, "healthy");

        // A reset time older than the rate limit itself says nothing: fall back to five hours.
        let mut stale = pooled("rate_limited", 10_000, Some(5_000));
        assert!(!heal_if_reset(&mut stale, 10_000 + 5 * 3600 - 1));
        assert!(heal_if_reset(&mut stale, 10_000 + 5 * 3600));

        let mut forbidden = pooled("forbidden", 0, None);
        assert!(!heal_if_reset(&mut forbidden, i64::MAX));
    }

    #[test]
    fn switching_refuses_accounts_without_a_sign_in_token() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(&directory.path().join("hub.sqlite3")).unwrap();
        database.save_antigravity_account(&pooled("healthy", 0, None)).unwrap();
        let error = switch_to_account(&Mutex::new(database), "antigravity:a@example.com").unwrap_err();
        assert!(error.to_string().contains("no saved sign-in token"), "{error}");
    }

    #[test]
    #[ignore]
    fn test_live_credential_contents() {
        if let Ok(Some(cred)) = read_system_antigravity_credential() {
            let client = Client::builder().timeout(Duration::from_secs(10)).build().unwrap();
            let user_info = client
                .get("https://www.googleapis.com/oauth2/v3/userinfo")
                .bearer_auth(&cred.token.access_token)
                .send()
                .ok()
                .and_then(|r| r.text().ok());
            println!("RAW USERINFO: {:?}", user_info);

            println!("TOKEN PROJECT: {:?}", cred.token.project_id);
        }
    }

    #[test]
    #[ignore]
    fn test_load_code_assist() {
        if let Ok(Some(cred)) = read_system_antigravity_credential() {
            let client = Client::builder().timeout(Duration::from_secs(10)).build().unwrap();
            let endpoints = [
                "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
                "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
            ];
            for endpoint in &endpoints {
                let resp = client
                    .post(*endpoint)
                    .bearer_auth(&cred.token.access_token)
                    .header("content-type", "application/json")
                    .header("user-agent", CLIENT_USER_AGENT)
                    .body(serde_json::json!({
                        "project": "aicode-consumers"
                    }).to_string())
                    .send()
                    .ok()
                    .and_then(|r| r.text().ok());
                println!("PROJECT QUOTA ENDPOINT {}: {:?}", endpoint, resp);
            }

        }
    }


    #[test]
    fn test_calculate_health_score_rules() {
        assert_eq!(calculate_health_score(Some(100.0), "rate_limited", 0), 0.0);
        assert_eq!(calculate_health_score(Some(100.0), "expired", 0), 0.0);
        let score_fresh = calculate_health_score(Some(80.0), "healthy", Utc::now().timestamp());
        assert!(score_fresh >= 68.0 && score_fresh <= 85.0);
        let score_rested = calculate_health_score(Some(80.0), "healthy", Utc::now().timestamp() - 86400);
        assert!(score_rested > score_fresh);
    }

    #[test]
    #[ignore = "requires running Antigravity app or local credentials"]
    fn test_detect_active_antigravity_session() {
        let session = detect_active_antigravity_session();
        println!("DETECTED SESSION: {:?}", session);
        assert!(session.email.is_some());
    }

    #[test]
    fn auto_switch_keeps_a_disabled_current_account_for_trigger_detection() {
        let database = Database::memory().unwrap();
        let mut current = pooled("healthy", 100, None);
        current.id = "current".into();
        current.email = "current@example.test".into();
        current.is_current = true;
        current.refresh_token = "fixture-current-refresh".into();
        let mut candidate = pooled("healthy", 200, None);
        candidate.id = "candidate".into();
        candidate.email = "candidate@example.test".into();
        candidate.quota_remaining_percent = Some(100.0);
        candidate.refresh_token = "fixture-candidate-refresh".into();
        database.save_antigravity_account(&current).unwrap();
        database.save_antigravity_account(&candidate).unwrap();
        let snapshot = crate::google_control::snapshot(&database.connection).unwrap();
        let mut metadata = snapshot.accounts.iter().find(|account| account.email == current.email).unwrap().metadata.clone();
        metadata.enabled = false;
        crate::google_control::save_account(&database.connection, metadata).unwrap();
        let (accounts, enabled) = auto_switch_pool(&database).unwrap();
        assert_eq!(accounts.len(), 2);
        assert!(accounts.iter().any(|account| account.is_current && account.id == "current"));
        assert!(!enabled.contains("current"));
        assert!(enabled.contains("candidate"));
        let candidates = accounts.iter().filter(|account| !account.is_current && enabled.contains(&account.id)).collect::<Vec<_>>();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "candidate");
    }
}
