use std::path::{Path, PathBuf};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

use crate::models::{AppStatus, ApplicationDefinition, RunningState};

pub struct ProcessDetector {
    system: System,
}

impl ProcessDetector {
    pub fn new() -> Self {
        Self {
            system: System::new(),
        }
    }

    pub fn scan(&mut self, apps: &[ApplicationDefinition]) -> Vec<AppStatus> {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_exe(sysinfo::UpdateKind::OnlyIfNotSet)
                .with_cmd(sysinfo::UpdateKind::Always),
        );

        apps.iter()
            .map(|app| {
                let matched = self.system.processes().values().find(|process| {
                    let name = process.name().to_string_lossy();
                    let executable = process.exe();
                    let command = process
                        .cmd()
                        .iter()
                        .map(|part| part.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(" ");
                    process_matches(app, &name, executable, &command)
                });
                let detected_path = matched
                    .and_then(|process| process.exe())
                    .map(|path| path.to_string_lossy().into_owned());
                let detected_name =
                    matched.map(|process| process.name().to_string_lossy().into_owned());
                let configured = resolve_executable(app);

                AppStatus {
                    id: app.id.clone(),
                    provider_id: app.provider_id.clone(),
                    display_name: app.display_name.clone(),
                    state: if matched.is_some() {
                        RunningState::Running
                    } else {
                        RunningState::NotRunning
                    },
                    detected_executable: detected_name,
                    executable_path: configured
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned()),
                    running_since: None,
                    current_session_seconds: 0,
                    today_seconds: 0,
                    week_seconds: 0,
                    last_used_at: None,
                    launchable: configured.is_some()
                        || detected_path
                            .as_deref()
                            .is_some_and(|path| Path::new(path).is_file()),
                }
            })
            .collect()
    }
}

pub fn process_matches(
    app: &ApplicationDefinition,
    process_name: &str,
    executable_path: Option<&Path>,
    command_line: &str,
) -> bool {
    let normalized = process_name.to_ascii_lowercase();
    // Seeded names carry Windows' `.exe`; macOS and Linux processes have none.
    let stem = normalized.strip_suffix(".exe").unwrap_or(&normalized);
    let executable = executable_path
        .map(|path| path.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let command = command_line.to_ascii_lowercase();
    let is_claude_process = stem == "claude";
    // Claude Desktop runs from Claude.app on macOS and AnthropicClaude on Windows. Every other
    // `claude` binary with a Unix path is the Claude Code CLI (native installer, npm, Homebrew).
    let in_desktop_bundle = executable.contains("/claude.app/");
    let is_claude_code = !in_desktop_bundle
        && (executable.contains("\\claude-code\\")
            || executable.contains("/claude-code/")
            || command.contains("@anthropic-ai\\claude-code")
            || command.contains("@anthropic-ai/claude-code")
            || command.contains("\\claude-code\\")
            || executable.starts_with('/'));
    if app.id == "claude-code" && is_claude_process {
        return is_claude_code;
    }
    if app.id == "claude" && is_claude_process {
        return !is_claude_code;
    }

    if app.process_names.iter().any(|candidate| {
        let candidate = candidate.to_ascii_lowercase();
        candidate.strip_suffix(".exe").unwrap_or(&candidate) == stem
    }) {
        return true;
    }

    // A macOS app's main process is the binary in `<Name>.app/Contents/MacOS/`, whatever it is
    // called (VS Code's is `Electron`); helpers live under `Contents/Frameworks/` and do not count.
    if mac_bundles(&app.id).iter().any(|bundle| {
        executable.contains(&format!("/{}/contents/macos/", bundle.to_ascii_lowercase()))
    }) {
        return true;
    }

    matches!(
        app.id.as_str(),
        "claude-code" | "gemini" | "codex" | "opencode"
    ) && stem == "node"
        && command.contains(match app.id.as_str() {
            "claude-code" => "claude-code",
            "gemini" => "gemini",
            "codex" => "@openai/codex",
            _ => "opencode",
        })
}

pub fn resolve_executable(app: &ApplicationDefinition) -> Option<PathBuf> {
    if let Some(path) = app
        .executable_path
        .as_ref()
        .map(PathBuf::from)
        .filter(|path| crate::platform::is_launchable(path))
    {
        return Some(path);
    }

    known_candidates(&app.id)
        .into_iter()
        .find(|path| crate::platform::is_launchable(path))
}

/// App bundle names of the desktop apps the hub knows, for macOS detection and launching.
fn mac_bundles(app_id: &str) -> &'static [&'static str] {
    match app_id {
        "codex" => &["Codex.app"],
        "chatgpt" => &["ChatGPT.app"],
        "claude" => &["Claude.app"],
        "antigravity" => &["Antigravity.app", "Antigravity IDE.app"],
        "cursor" => &["Cursor.app"],
        "vscode" => &["Visual Studio Code.app"],
        "kimi" => &["Kimi.app"],
        _ => &[],
    }
}

#[cfg(target_os = "macos")]
fn known_candidates(app_id: &str) -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = crate::platform::home_dir() {
        roots.push(home.join("Applications"));
    }
    roots
        .iter()
        .flat_map(|root| mac_bundles(app_id).iter().map(move |bundle| root.join(bundle)))
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn known_candidates(app_id: &str) -> Vec<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let programs = std::env::var_os("ProgramFiles").map(PathBuf::from);
    let mut paths = Vec::new();
    let relative: &[&str] = match app_id {
        "codex" => &[
            "Programs/Codex/Codex.exe",
            "Programs/OpenAI Codex/Codex.exe",
        ],
        "chatgpt" => &["Programs/ChatGPT/ChatGPT.exe"],
        "claude" => &["Programs/Claude/Claude.exe", "AnthropicClaude/Claude.exe"],
        "antigravity" => &[
            "Programs/Antigravity/Antigravity.exe",
            "Programs/Antigravity IDE/Antigravity IDE.exe",
        ],
        "cursor" => &["Programs/cursor/Cursor.exe"],
        "vscode" => &["Programs/Microsoft VS Code/Code.exe"],
        "kimi" => &["Programs/Kimi/Kimi.exe"],
        _ => &[],
    };
    if let Some(root) = local {
        paths.extend(relative.iter().map(|path| root.join(path)));
    }
    if let Some(root) = programs {
        let global: &[&str] = match app_id {
            "vscode" => &["Microsoft VS Code/Code.exe"],
            "cursor" => &["Cursor/Cursor.exe"],
            _ => &[],
        };
        paths.extend(global.iter().map(|path| root.join(path)));
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(id: &str, names: &[&str]) -> ApplicationDefinition {
        ApplicationDefinition {
            id: id.into(),
            provider_id: None,
            display_name: id.into(),
            executable_path: None,
            launch_arguments: vec![],
            process_names: names.iter().map(|name| (*name).into()).collect(),
        }
    }

    #[test]
    fn process_names_match_case_insensitively_but_not_partially() {
        let app = definition("codex", &["Codex.exe"]);
        assert!(process_matches(&app, "CODEX.EXE", None, ""));
        assert!(!process_matches(&app, "codex-helper.exe", None, ""));
    }

    #[test]
    fn cli_wrappers_are_detected_from_node_command_lines() {
        let app = definition("claude-code", &["claude-code.exe"]);
        assert!(process_matches(
            &app,
            "node.exe",
            None,
            r#"node C:\npm\node_modules\@anthropic-ai\claude-code\cli.js"#
        ));
        assert!(!process_matches(&app, "node.exe", None, "node vite.js"));
    }

    #[test]
    fn claude_desktop_and_code_are_distinguished_by_executable_path() {
        let desktop = definition("claude", &["claude.exe"]);
        let code = definition("claude-code", &["claude-code.exe"]);
        let desktop_path = Path::new(r"C:\Users\me\AppData\Local\AnthropicClaude\app-2\claude.exe");
        let code_path = Path::new(r"C:\Users\me\AppData\Roaming\Claude\claude-code\2\claude.exe");

        assert!(process_matches(
            &desktop,
            "claude.exe",
            Some(desktop_path),
            ""
        ));
        assert!(!process_matches(
            &code,
            "claude.exe",
            Some(desktop_path),
            ""
        ));
        assert!(process_matches(&code, "claude.exe", Some(code_path), ""));
        assert!(!process_matches(
            &desktop,
            "claude.exe",
            Some(code_path),
            ""
        ));
    }

    #[test]
    fn macos_processes_match_by_name_and_app_bundle() {
        let codex = definition("codex", &["codex.exe"]);
        assert!(process_matches(&codex, "Codex", Some(Path::new("/Applications/Codex.app/Contents/MacOS/Codex")), ""));

        let vscode = definition("vscode", &["code.exe"]);
        let main = Path::new("/Applications/Visual Studio Code.app/Contents/MacOS/Electron");
        let helper = Path::new(
            "/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper (Renderer).app/Contents/MacOS/Code Helper (Renderer)",
        );
        assert!(process_matches(&vscode, "Electron", Some(main), ""));
        assert!(!process_matches(&vscode, "Code Helper (Renderer)", Some(helper), ""));
        assert!(!process_matches(&codex, "Electron", Some(main), ""));
    }

    #[test]
    fn macos_claude_desktop_and_cli_are_distinguished() {
        let desktop = definition("claude", &["claude.exe"]);
        let code = definition("claude-code", &["claude-code.exe"]);
        let app = Path::new("/Applications/Claude.app/Contents/MacOS/Claude");
        let cli = Path::new("/Users/me/.local/share/claude/versions/2.1.0");

        assert!(process_matches(&desktop, "Claude", Some(app), ""));
        assert!(!process_matches(&code, "Claude", Some(app), ""));
        assert!(process_matches(&code, "claude", Some(cli), "claude"));
        assert!(!process_matches(&desktop, "claude", Some(cli), "claude"));
        assert!(process_matches(&code, "node", None, "node /opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js"));
    }

    #[test]
    #[ignore = "requires running Claude Desktop and Antigravity apps"]
    fn live_windows_desktop_app_detection() {
        let apps = vec![
            definition("claude", &["claude.exe"]),
            definition("antigravity", &["antigravity.exe", "antigravity ide.exe"]),
        ];
        let statuses = ProcessDetector::new().scan(&apps);
        for status in statuses {
            assert_eq!(status.state, RunningState::Running, "{}", status.id);
        }
    }
}
