mod anthropic;
pub(crate) mod google;
mod openai;
mod opencode;

use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use crate::models::{ConnectionStatus, ProviderCapabilities, ProviderSummary, UsageValue};

pub(super) fn recent_jsonl_files(root: &Path, cutoff: i64) -> Vec<PathBuf> {
    let mut directories = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|value| value == "jsonl")
                && entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                    .is_some_and(|modified| modified.as_secs() as i64 >= cutoff)
            {
                files.push(path);
            }
        }
    }
    files
}

pub trait ProviderAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn detect(&self) -> bool {
        false
    }
    fn get_accounts(&self) -> Vec<String> {
        Vec::new()
    }
    fn get_usage(&self) -> Vec<UsageValue> {
        vec![UsageValue::unavailable("usage", "Usage information")]
    }
    fn get_limits(&self) -> Vec<UsageValue> {
        Vec::new()
    }
    fn get_reset_times(&self) -> Vec<i64> {
        Vec::new()
    }
    fn get_status(&self) -> ConnectionStatus {
        ConnectionStatus::NotConnected
    }
    fn launch_app(&self) -> bool {
        false
    }
    fn refresh_auth(&self) -> bool {
        false
    }
    fn capabilities(&self) -> ProviderCapabilities;
}

struct UnavailableProvider {
    id: &'static str,
    name: &'static str,
}

impl ProviderAdapter for UnavailableProvider {
    fn id(&self) -> &'static str {
        self.id
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            local_session_detection: true,
            app_launch: true,
            ..Default::default()
        }
    }
}

pub fn summaries() -> Vec<ProviderSummary> {
    adapters()
        .into_iter()
        .map(|adapter| {
            let _ = (
                adapter.detect(),
                adapter.get_accounts(),
                adapter.get_limits(),
                adapter.get_reset_times(),
                adapter.launch_app(),
                adapter.refresh_auth(),
            );
            ProviderSummary {
                id: adapter.id().into(),
                name: adapter.name().into(),
                account_key: None,
                account_label: "Not connected".into(),
                plan: None,
                connection_status: adapter.get_status(),
                capabilities: adapter.capabilities(),
                usage: adapter.get_usage(),
                usage_allowed: None,
                last_refresh: None,
                error: None,
            }
        })
        .collect()
}

fn adapters() -> Vec<Box<dyn ProviderAdapter>> {
    vec![
        Box::new(OpenAiPlaceholder),
        Box::new(UnavailableProvider {
            id: "anthropic",
            name: "Anthropic / Claude",
        }),
        Box::new(GooglePlaceholder),
        Box::new(UnavailableProvider {
            id: "opencode",
            name: "OpenCode",
        }),
        Box::new(UnavailableProvider {
            id: "kimi",
            name: "Kimi",
        }),
        Box::new(UnavailableProvider {
            id: "github",
            name: "GitHub",
        }),
    ]
}

struct OpenAiPlaceholder;

impl ProviderAdapter for OpenAiPlaceholder {
    fn id(&self) -> &'static str {
        "openai"
    }

    fn name(&self) -> &'static str {
        "ChatGPT / Codex"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            usage_percentage: true,
            reset_time: true,
            local_session_detection: true,
            oauth: true,
            multiple_accounts: true,
            app_launch: true,
            rate_limit_status: true,
            ..Default::default()
        }
    }
}

struct GooglePlaceholder;

impl ProviderAdapter for GooglePlaceholder {
    fn id(&self) -> &'static str {
        "google"
    }

    fn name(&self) -> &'static str {
        "Google / Antigravity"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            usage_percentage: true,
            reset_time: true,
            local_session_detection: true,
            oauth: true,
            multiple_accounts: true,
            app_launch: true,
            rate_limit_status: true,
            ..Default::default()
        }
    }
}

pub fn refresh_summaries(previous: &[ProviderSummary]) -> Vec<ProviderSummary> {
    let mut summaries = summaries();
    replace_with_live(
        &mut summaries,
        previous,
        "anthropic",
        "Claude account unavailable",
        anthropic::summary(),
    );
    replace_with_live(
        &mut summaries,
        previous,
        "openai",
        "Codex usage unavailable",
        openai::summary(),
    );
    replace_with_live(
        &mut summaries,
        previous,
        "google",
        "Antigravity usage unavailable",
        google::summary(),
    );
    replace_with_live(
        &mut summaries,
        previous,
        "opencode",
        "OpenCode usage unavailable",
        opencode::summary(),
    );
    summaries
}

fn replace_with_live(
    summaries: &mut [ProviderSummary],
    previous: &[ProviderSummary],
    id: &str,
    unavailable_label: &str,
    result: crate::error::Result<ProviderSummary>,
) {
    let live = match result {
        Ok(summary) => summary,
        Err(error) => {
            let message = error.to_string();
            if let Some(cached) = previous.iter().find(|provider| {
                provider.id == id && provider.usage.iter().any(|usage| usage.value.is_some())
            }) {
                let mut cached = cached.clone();
                cached.error = Some(format!("Latest refresh failed: {message}"));
                cached
            } else {
                let mut unavailable = summaries
                    .iter()
                    .find(|provider| provider.id == id)
                    .cloned()
                    .expect("provider placeholder is present");
                unavailable.connection_status = ConnectionStatus::Error;
                unavailable.account_label = unavailable_label.into();
                unavailable.error = Some(message);
                unavailable
            }
        }
    };
    if let Some(slot) = summaries.iter_mut().find(|provider| provider.id == id) {
        *slot = live;
    }
}
