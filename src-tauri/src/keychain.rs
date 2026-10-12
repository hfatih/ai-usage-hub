//! macOS Keychain access through `/usr/bin/security`.
//!
//! Antigravity's language server stores its Google sign-in with Go's `go-keyring`, which drives
//! the same tool: values are written as `go-keyring-base64:<base64>` through `security -i` (so the
//! secret never appears in a process list) and read with `find-generic-password -w`. Matching that
//! format keeps both programs able to read what the other wrote, and because `/usr/bin/security`
//! created the items, reading them does not raise a Keychain permission prompt.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

const BASE64_PREFIX: &str = "go-keyring-base64:";
const HEX_PREFIX: &str = "go-keyring-encoded:";
/// `security -i` reads commands into a fixed buffer; longer lines are cut off.
const MAX_COMMAND_LEN: usize = 4096;

#[cfg(target_os = "macos")]
mod cli {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    const SECURITY: &str = "/usr/bin/security";
    /// `errSecItemNotFound`, which `security` reports as its exit status.
    const ITEM_NOT_FOUND: i32 = 44;

    /// Reads a generic password. `Ok(None)` means the item does not exist; any other failure
    /// (locked Keychain, denied access) is an error so callers never mistake it for "missing".
    pub fn read(service: &str, account: &str) -> Result<Option<String>, String> {
        let output = Command::new(SECURITY)
            .args(["find-generic-password", "-s", service, "-a", account, "-w"])
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("could not run {SECURITY}: {error}"))?;
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            return super::decode(text.trim()).map(Some);
        }
        if output.status.code() == Some(ITEM_NOT_FOUND) {
            return Ok(None);
        }
        Err(format!(
            "Keychain read failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }

    /// Creates or replaces a generic password, then reads it back to confirm the write.
    pub fn write(service: &str, account: &str, secret: &str) -> Result<(), String> {
        let command = super::add_command(service, account, secret)?;
        let mut child = Command::new(SECURITY)
            .arg("-i")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("could not run {SECURITY}: {error}"))?;
        child
            .stdin
            .take()
            .ok_or_else(|| "Keychain input is unavailable".to_owned())?
            .write_all(command.as_bytes())
            .map_err(|error| format!("Keychain write failed: {error}"))?;
        let output = child
            .wait_with_output()
            .map_err(|error| format!("Keychain write failed: {error}"))?;
        // `security -i` can exit successfully even when the command inside failed.
        match read(service, account) {
            Ok(Some(stored)) if stored == secret => Ok(()),
            _ => Err(format!(
                "Keychain write was not saved: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )),
        }
    }
}

#[cfg(target_os = "macos")]
pub use cli::{read, write};

/// Turns what `find-generic-password -w` printed back into the stored secret.
fn decode(value: &str) -> Result<String, String> {
    let bytes = if let Some(encoded) = value.strip_prefix(BASE64_PREFIX) {
        base64_decode(encoded).ok_or("invalid base64 in Keychain item")?
    } else if let Some(encoded) = value.strip_prefix(HEX_PREFIX) {
        hex_decode(encoded).ok_or("invalid hex in Keychain item")?
    } else {
        return Ok(value.to_owned());
    };
    String::from_utf8(bytes).map_err(|_| "Keychain item is not UTF-8".to_owned())
}

/// The `security -i` line that stores `secret`, encoded the way `go-keyring` does.
fn add_command(service: &str, account: &str, secret: &str) -> Result<String, String> {
    let value = format!("{BASE64_PREFIX}{}", base64_encode(secret.as_bytes()));
    let command = format!(
        "add-generic-password -U -s {} -a {} -w {}\n",
        quote(service)?,
        quote(account)?,
        quote(&value)?
    );
    if command.len() > MAX_COMMAND_LEN {
        return Err("value is too large to store in the Keychain".into());
    }
    Ok(command)
}

fn quote(value: &str) -> Result<String, String> {
    let safe = |c: char| c.is_ascii_alphanumeric() || "@%+=:,./-_".contains(c);
    if !value.is_empty() && value.chars().all(safe) {
        Ok(value.to_owned())
    } else if value.contains(['\'', '\n', '\r']) {
        Err("Keychain names cannot contain quotes or line breaks".into())
    } else {
        Ok(format!("'{value}'"))
    }
}

const BASE64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                out.push(BASE64_TABLE[(n >> shift) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(value: &str) -> Option<Vec<u8>> {
    let value = value.trim_end_matches('=');
    let mut out = Vec::with_capacity(value.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0;
    for byte in value.bytes() {
        let digit = BASE64_TABLE.iter().position(|&c| c == byte)? as u32;
        buffer = (buffer << 6) | digit;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| value.get(index..index + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip_through_the_go_keyring_encodings() {
        for sample in ["", "a", "ab", "abc", "{\"token\":{\"access_token\":\"ya29.x\"}}", "çğüş"] {
            let encoded = format!("{BASE64_PREFIX}{}", base64_encode(sample.as_bytes()));
            assert_eq!(decode(&encoded).unwrap(), sample);
        }
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(decode("go-keyring-encoded:68656c6c6f").unwrap(), "hello");
        assert_eq!(decode("plain value").unwrap(), "plain value");
        assert!(decode("go-keyring-base64:@@").is_err());
    }

    #[test]
    fn add_commands_are_single_safe_lines() {
        let command = add_command("gemini", "antigravity", "line one\nline 'two'").unwrap();
        assert!(command.starts_with("add-generic-password -U -s gemini -a antigravity -w go-keyring-base64:"));
        assert_eq!(command.matches('\n').count(), 1);
        assert!(!command.contains("line"));
        assert_eq!(quote("AI Usage Hub").unwrap(), "'AI Usage Hub'");
        assert!(quote("it's").is_err());
        assert!(add_command("s", "a", &"x".repeat(MAX_COMMAND_LEN)).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "writes to the login Keychain"]
    fn live_keychain_round_trip() {
        let account = format!("test-{}", std::process::id());
        write("com.aiusagehub.desktop.test", &account, "secret value").unwrap();
        assert_eq!(read("com.aiusagehub.desktop.test", &account).unwrap().as_deref(), Some("secret value"));
        assert_eq!(read("com.aiusagehub.desktop.test", "missing-account").unwrap(), None);
    }
}
