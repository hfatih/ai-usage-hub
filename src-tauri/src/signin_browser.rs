use crate::error::{HubError, Result};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Pending,
    Success,
    Failed,
}

#[derive(Clone, Copy)]
pub(crate) enum LoginProvider {
    Codex,
    Claude,
}

impl LoginProvider {
    fn route(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
        }
    }
}

pub(crate) struct LoginPage {
    pub url: String,
    state: Arc<Mutex<(State, Instant)>>,
}

impl LoginPage {
    pub(crate) fn start(
        login_id: &str,
        auth_url: String,
        language: String,
        provider: LoginProvider,
    ) -> Result<Self> {
        if login_id.is_empty() || !login_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Err(HubError::Operation(
                "The sign-in page received an invalid login ID.".into(),
            ));
        }
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let path = format!("/{}/login/{login_id}", provider.route());
        let url = format!("http://{address}{path}");
        let state = Arc::new(Mutex::new((State::Pending, Instant::now())));
        let observed = Arc::clone(&state);
        std::thread::spawn(move || loop {
            let Ok((status, since)) = observed.lock().map(|state| *state) else {
                break;
            };
            let lifetime = if status == State::Pending { 315 } else { 90 };
            if since.elapsed() >= Duration::from_secs(lifetime) {
                break;
            }
            match listener.accept() {
                Ok((stream, _)) => respond(
                    stream,
                    &path,
                    &address.to_string(),
                    status,
                    &auth_url,
                    &language,
                    provider,
                ),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(75))
                }
                Err(_) => break,
            }
        });
        Ok(Self { url, state })
    }

    pub(crate) fn complete(&self, succeeded: bool) {
        if let Ok(mut state) = self.state.lock() {
            *state = (
                if succeeded {
                    State::Success
                } else {
                    State::Failed
                },
                Instant::now(),
            );
        }
    }
}

impl Drop for LoginPage {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            if state.0 == State::Pending {
                *state = (State::Failed, Instant::now());
            }
        }
    }
}

fn page(status: State, auth_url: &str, lang: &str, provider: LoginProvider) -> String {
    if status == State::Success {
        return crate::signin_page::success_page(provider.title(), lang);
    }
    let tr = lang == "tr";
    let (mut title, mut message, mut hint) = match (status, tr) {
        (State::Pending, true) => (
            "ChatGPT hesabını ekle",
            "Giriş bu tarayıcıda yeni bir sekmede açılır. Bu sekme sonucu otomatik gösterir.",
            "İkinci hesabı eklemek için farklı bir e-posta seç.",
        ),
        (State::Pending, false) => (
            "Add a ChatGPT account",
            "Sign-in opens in a new tab in this browser. This tab shows the result automatically.",
            "Choose a different email to add another account.",
        ),
        (_, true) => (
            "Giriş tamamlanmadı",
            "Giriş iptal edildi, süresi doldu veya doğrulanamadı.",
            "AI Usage Hub’dan yeni bir giriş bağlantısı al.",
        ),
        (_, false) => (
            "Sign-in incomplete",
            "Sign-in was cancelled, expired, or could not be verified.",
            "Get a new sign-in link from AI Usage Hub.",
        ),
    };
    if matches!(provider, LoginProvider::Claude) && status == State::Pending {
        (title, message, hint) = if tr {
            ("Claude hesabını ekle", "Claude’un sayfasında Google ile devam et. Giriş bu tarayıcıda yeni sekmede açılır.", "Giriş doğrulanınca bu sekme sonucu gösterecek.")
        } else {
            ("Add a Claude account", "Choose Continue with Google on Claude’s page. Sign-in opens in a new tab in this browser.", "This tab shows the result after Hub verifies sign-in.")
        };
    }
    let action = if status == State::Pending {
        let url = auth_url
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let button = match (provider, tr) {
            (LoginProvider::Codex, true) => "ChatGPT ile girişe devam et",
            (LoginProvider::Codex, false) => "Continue with ChatGPT",
            (LoginProvider::Claude, true) => "Claude giriş sayfasını aç",
            (LoginProvider::Claude, false) => "Open Claude sign-in",
        };
        format!(
            r#"<p style="margin-top:18px"><a style="color:#ffe6ef;text-underline-offset:4px" href="{url}" target="_blank" rel="noopener noreferrer">{button}</a></p>"#
        )
    } else {
        String::new()
    };
    let refresh = if status == State::Pending {
        r#"<meta http-equiv="refresh" content="2">"#
    } else {
        ""
    };
    include_str!("oauth_callback.html")
        .replace(
            "<html lang=\"tr\">",
            if tr {
                "<html lang=\"tr\">"
            } else {
                "<html lang=\"en\">"
            },
        )
        .replace("</head>", &format!("{refresh}</head>"))
        .replace("{{ICON}}", "✦")
        .replace("{{TITLE}}", title)
        .replace("{{MESSAGE}}", message)
        .replace("{{HINT}}", hint)
        .replace(
            r#"<p class="hint">"#,
            &format!(r#"{action}<p class="hint">"#),
        )
        .replace("{{SCRIPT}}", "")
}

fn respond(
    mut stream: TcpStream,
    path: &str,
    host: &str,
    state: State,
    auth_url: &str,
    lang: &str,
    provider: LoginProvider,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let mut reader = BufReader::new((&mut stream).take(8192));
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let matches_path = line.split_ascii_whitespace().take(2).eq(["GET", path]);
    let mut matches_host = false;
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("host") && value.trim() == host {
                matches_host = true;
            }
        }
    }
    let (code, body) = if matches_path && matches_host {
        ("200 OK", page(state, auth_url, lang, provider))
    } else {
        ("404 Not Found", String::new())
    };
    let _ = write!(stream, "HTTP/1.1 {code}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'\r\nConnection: close\r\n\r\n{body}", body.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claude_page_guides_google_and_stays_open_until_claude_verifies() {
        let auth = "https://claude.com/cai/oauth/authorize?state=example";
        let pending = page(State::Pending, auth, "en", LoginProvider::Claude);
        assert!(pending.contains("Continue with Google"));
        assert!(pending.contains("target=\"_blank\""));
        assert!(pending.contains("http-equiv=\"refresh\""));
        let result = page(State::Success, auth, "tr", LoginProvider::Claude);
        assert!(result.contains("Claude Code"));
        assert!(result.contains("Giriş tamamlandı"));
        assert!(!result.contains(auth));
        let failed = page(State::Failed, auth, "en", LoginProvider::Claude);
        assert!(!failed.contains(auth));
    }

    #[test]
    fn local_page_stays_on_loopback_and_changes_after_verification() {
        let page = LoginPage::start(
            "dead-beef",
            "https://auth.openai.com/oauth/authorize?x=demo&y=sample".into(),
            "tr".into(),
            LoginProvider::Codex,
        )
        .unwrap();
        assert!(page.url.starts_with("http://127.0.0.1:"));
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let pending = client
            .get(&page.url)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .unwrap();
        assert!(pending.contains("target=\"_blank\""));
        assert!(pending.contains("&amp;y=sample"));
        assert!(!pending.contains("Giriş tamamlandı"));
        page.complete(true);
        let done = client
            .get(&page.url)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .unwrap();
        assert!(done.contains("Giriş tamamlandı"));
        assert!(done.contains("class=\"star s12\""));
        assert!(!done.contains("auth.openai.com"));
        assert_eq!(
            client
                .get(format!("{}/unknown", page.url))
                .send()
                .unwrap()
                .status(),
            404
        );
    }
}
