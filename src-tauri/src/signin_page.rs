use crate::HubState;
use std::process::Command;

pub(crate) fn language(state: &HubState) -> String {
    state
        .database
        .lock()
        .ok()
        .and_then(|db| db.settings().ok())
        .map(|settings| settings.language)
        .unwrap_or_default()
}

pub(crate) fn success_page(provider: &'static str, lang: &str) -> String {
    let (lang, title, message, hint) = if lang == "tr" {
        (
            "tr",
            "Giriş tamamlandı",
            "Hesabın yerel profiline kaydedildi.",
            "Bu sekmeyi kapatıp AI Usage Hub’a dönebilirsin.",
        )
    } else {
        (
            "en",
            "Sign-in complete",
            "Your account is saved in its local profile.",
            "You can close this tab and return to AI Usage Hub.",
        )
    };
    include_str!("oauth_callback.html")
        .replace("<html lang=\"tr\">", &format!("<html lang=\"{lang}\">"))
        .replace("{{ICON}}", "✦")
        .replace("{{TITLE}}", &format!("{provider} · {title}"))
        .replace("{{MESSAGE}}", message)
        .replace("{{HINT}}", hint)
        .replace("{{SCRIPT}}", "")
}

pub(crate) fn open_browser(url: &str) -> crate::error::Result<()> {
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("rundll32");
        command
            .args(["url.dll,FileProtocolHandler", url])
            .creation_flags(0x0800_0000);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };
    let mut browser = command.spawn().map_err(|_| {
        crate::error::HubError::Operation(
            "Could not open the sign-in page. Check your default browser.".into(),
        )
    })?;
    std::thread::spawn(move || {
        let _ = browser.wait();
    });
    Ok(())
}

pub(crate) fn copy_url(url: &str) -> crate::error::Result<()> {
    fn unavailable() -> crate::error::HubError {
        crate::error::HubError::Operation("The Windows clipboard is unavailable.".into())
    }
    #[cfg(windows)]
    {
        use std::{io::Write, os::windows::process::CommandExt, path::PathBuf, process::Stdio};
        let system_root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(unavailable)?;
        let mut child = Command::new(system_root.join("System32").join("clip.exe"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|_| unavailable())?;
        let bytes: Vec<u8> = std::iter::once(0xfeff)
            .chain(url.encode_utf16())
            .flat_map(u16::to_le_bytes)
            .collect();
        let written = child
            .stdin
            .take()
            .is_some_and(|mut pipe| pipe.write_all(&bytes).is_ok());
        if written && child.wait().is_ok_and(|status| status.success()) {
            return Ok(());
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    #[cfg(not(windows))]
    let _ = url;
    Err(unavailable())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_provider_pages_use_the_starlit_template_and_locale() {
        for provider in ["Codex", "Claude Code"] {
            let page = success_page(provider, "en");
            assert!(page.contains(provider));
            assert!(page.contains("class=\"star s12\""));
            assert!(page.contains("<html lang=\"en\">"));
            assert!(!page.contains("{{"));
            assert!(success_page(provider, "tr").contains("Giriş tamamlandı"));
        }
    }
}
