//! One-click connection of coding tools to the local proxy by editing their own config file:
//! Claude Code (`~/.claude/settings.json`) and OpenCode (`~/.config/opencode/opencode.json`).
//!
//! Only the hub's own entries are added or removed; every other setting and the key order of the
//! file are kept. Before the first change the file is copied next to itself with a
//! `.hub-backup` suffix, and that copy is never overwritten. A file that is not plain JSON (for
//! example one with comments) is left untouched and reported as an error.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::error::{HubError, Result};

const LOCAL_PREFIX: &str = "http://127.0.0.1:";
const OPENCODE_PROVIDER: &str = "ai-usage-hub";
const CLAUDE_KEYS: [&str; 2] = ["ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN"];

/// Whether each supported tool currently points at the hub's proxy.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliConfigStatus {
    pub claude: bool,
    pub opencode: bool,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
}

fn config_path(tool: &str) -> Result<PathBuf> {
    let home = home().ok_or_else(|| HubError::Operation("the user folder could not be found".into()))?;
    match tool {
        "claude" => Ok(home.join(".claude").join("settings.json")),
        "opencode" => Ok(home.join(".config").join("opencode").join("opencode.json")),
        other => Err(HubError::Configuration(format!("unsupported tool '{other}'"))),
    }
}

/// Reads both tools' config files and reports which ones carry the hub's entries. A missing or
/// unreadable file counts as not connected.
pub fn status() -> CliConfigStatus {
    let applied = |tool: &str| config_path(tool).ok().and_then(|path| read(&path).ok()).is_some_and(|config| is_applied(tool, &config));
    CliConfigStatus { claude: applied("claude"), opencode: applied("opencode") }
}

/// Points `tool` (`claude` or `opencode`) at the proxy at `base_url` with `api_key`. `models` are
/// the upstream model ids offered to OpenCode; Claude Code ignores them.
pub fn apply(tool: &str, base_url: &str, api_key: &str, models: &[String]) -> Result<()> {
    apply_at(tool, &config_path(tool)?, base_url, api_key, models)
}

/// Removes the hub's entries from `tool`'s config. Other settings stay as they are.
pub fn remove(tool: &str) -> Result<()> {
    remove_at(tool, &config_path(tool)?)
}

fn read(path: &Path) -> Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let text = fs::read_to_string(path)?;
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(text.trim_start_matches('\u{feff}')) {
        Ok(Value::Object(config)) => Ok(config),
        _ => Err(HubError::Configuration(format!(
            "{} is not plain JSON, so it was left unchanged; add the settings by hand",
            path.display()
        ))),
    }
}

fn write(path: &Path, config: Map<String, Value>) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let backup = PathBuf::from(format!("{}.hub-backup", path.display()));
    if path.exists() && !backup.exists() {
        fs::copy(path, backup)?;
    }
    let text = serde_json::to_string_pretty(&Value::Object(config)).map_err(|error| HubError::Operation(error.to_string()))?;
    fs::write(path, text + "\n")?;
    Ok(())
}

fn is_applied(tool: &str, config: &Map<String, Value>) -> bool {
    match tool {
        "claude" => config
            .get("env")
            .and_then(|env| env.get(CLAUDE_KEYS[0]))
            .and_then(Value::as_str)
            .is_some_and(|url| url.starts_with(LOCAL_PREFIX)),
        _ => config.get("provider").and_then(|providers| providers.get(OPENCODE_PROVIDER)).is_some(),
    }
}

fn object_entry<'a>(config: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let entry = config.entry(key.to_owned()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    entry.as_object_mut().expect("entry was just made an object")
}

fn apply_at(tool: &str, path: &Path, base_url: &str, api_key: &str, models: &[String]) -> Result<()> {
    let mut config = read(path)?;
    if tool == "claude" {
        let env = object_entry(&mut config, "env");
        env.insert(CLAUDE_KEYS[0].into(), base_url.into());
        env.insert(CLAUDE_KEYS[1].into(), api_key.into());
    } else {
        let listed: Map<String, Value> = models.iter().map(|id| (id.clone(), json!({ "name": id }))).collect();
        object_entry(&mut config, "provider").insert(
            OPENCODE_PROVIDER.into(),
            json!({
                "npm": "@ai-sdk/openai-compatible",
                "name": "AI Usage Hub",
                "options": { "baseURL": format!("{base_url}/v1"), "apiKey": api_key },
                "models": listed,
            }),
        );
    }
    write(path, config)
}

fn remove_at(tool: &str, path: &Path) -> Result<()> {
    let mut config = read(path)?;
    if !is_applied(tool, &config) {
        return Ok(());
    }
    let section = if tool == "claude" { "env" } else { "provider" };
    let keys: &[&str] = if tool == "claude" { &CLAUDE_KEYS } else { &[OPENCODE_PROVIDER] };
    let entries = object_entry(&mut config, section);
    for key in keys {
        entries.shift_remove(*key);
    }
    if entries.is_empty() {
        config.shift_remove(section);
    }
    write(path, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_settings_keep_everything_else_and_restore_cleanly() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");
        fs::write(&path, r#"{"zeta": 1, "env": {"KEEP": "yes"}, "alpha": [2]}"#).unwrap();

        apply_at("claude", &path, "http://127.0.0.1:8046", "sk-hub-test", &[]).unwrap();
        let applied = read(&path).unwrap();
        assert!(is_applied("claude", &applied));
        assert_eq!(applied["env"]["KEEP"], "yes");
        assert_eq!(applied["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-hub-test");
        assert_eq!(applied.keys().collect::<Vec<_>>(), ["zeta", "env", "alpha"]);
        assert!(fs::read_to_string(folder.path().join("settings.json.hub-backup")).unwrap().contains(r#""zeta": 1"#));

        remove_at("claude", &path).unwrap();
        let removed = read(&path).unwrap();
        assert!(!is_applied("claude", &removed));
        assert_eq!(removed["env"], json!({"KEEP": "yes"}));
    }

    #[test]
    fn opencode_gets_a_provider_and_commented_files_are_refused() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("nested").join("opencode.json");
        apply_at("opencode", &path, "http://127.0.0.1:8046", "sk-hub-test", &["gemini-x".to_owned()]).unwrap();
        let config = read(&path).unwrap();
        assert_eq!(config["provider"][OPENCODE_PROVIDER]["options"]["baseURL"], "http://127.0.0.1:8046/v1");
        assert!(config["provider"][OPENCODE_PROVIDER]["models"].get("gemini-x").is_some());
        remove_at("opencode", &path).unwrap();
        assert!(read(&path).unwrap().get("provider").is_none());

        let commented = folder.path().join("commented.json");
        fs::write(&commented, "{ // note\n \"a\": 1 }").unwrap();
        assert!(apply_at("opencode", &commented, "http://127.0.0.1:8046", "k", &[]).is_err());
        assert!(fs::read_to_string(&commented).unwrap().contains("// note"));
    }
}
