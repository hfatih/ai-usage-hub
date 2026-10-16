mod anthropic;
mod copilot;
mod cursor;
mod gemini_cli;
pub(crate) mod google;
pub(crate) mod keyed;
mod openai;
mod opencode;

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant, UNIX_EPOCH},
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
            id: "gemini_cli",
            name: "Gemini CLI",
        }),
        Box::new(UnavailableProvider {
            id: "opencode",
            name: "OpenCode",
        }),
        Box::new(UnavailableProvider {
            id: "kimi",
            name: keyed::name("kimi"),
        }),
        Box::new(UnavailableProvider {
            id: "github",
            name: "GitHub Copilot",
        }),
        Box::new(UnavailableProvider {
            id: "cursor",
            name: "Cursor",
        }),
        Box::new(UnavailableProvider {
            id: "grok",
            name: keyed::name("grok"),
        }),
        Box::new(UnavailableProvider {
            id: "deepseek",
            name: keyed::name("deepseek"),
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

/// How long a successful answer from a remote account endpoint is reused by background refreshes.
const REMOTE_TTL: Duration = Duration::from_secs(300);
static REMOTE_CACHE: LazyLock<Mutex<HashMap<String, (Instant, ProviderSummary)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Runs `fetch` for a provider whose numbers come from a remote account endpoint, at most once
/// per [`REMOTE_TTL`] unless `force` is set. Quotas and balances move slowly, so the once-a-minute
/// background refresh does not need to call GitHub, Cursor or a billing API every time. Failures
/// are never cached, so signing in or fixing a key shows up on the next refresh. Saving a key
/// always forces a refresh, so an answer fetched with a replaced key is not served.
fn throttled(provider_id: &str, force: bool, fetch: impl FnOnce() -> crate::error::Result<ProviderSummary>) -> crate::error::Result<ProviderSummary> {
    if !force {
        if let Some((fetched_at, summary)) = REMOTE_CACHE.lock().ok().and_then(|cache| cache.get(provider_id).cloned()) {
            if fetched_at.elapsed() < REMOTE_TTL {
                return Ok(summary);
            }
        }
    }
    let summary = fetch()?;
    if let Ok(mut cache) = REMOTE_CACHE.lock() {
        cache.insert(provider_id.to_owned(), (Instant::now(), summary.clone()));
    }
    Ok(summary)
}

/// Refreshes every provider. `keys` holds the API keys the user saved for key-based providers;
/// one without a key stays "not connected" and is not contacted. `force` (a refresh the user
/// asked for) bypasses the reuse of recent answers from remote account endpoints.
pub fn refresh_summaries(previous: &[ProviderSummary], keys: &HashMap<String, String>, force: bool) -> Vec<ProviderSummary> {
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
    replace_with_live(
        &mut summaries,
        previous,
        "gemini_cli",
        "Gemini CLI not signed in",
        throttled("gemini_cli", force, gemini_cli::summary),
    );
    replace_with_live(
        &mut summaries,
        previous,
        "github",
        "Copilot not signed in",
        throttled("github", force, copilot::summary),
    );
    replace_with_live(
        &mut summaries,
        previous,
        "cursor",
        "Cursor not signed in",
        throttled("cursor", force, cursor::summary),
    );
    for provider_id in keyed::KEYED_PROVIDERS {
        if let Some(key) = keys.get(provider_id) {
            replace_with_live(
                &mut summaries,
                previous,
                provider_id,
                "API key not accepted",
                throttled(provider_id, force, || keyed::summary(provider_id, key)),
            );
        }
    }
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
