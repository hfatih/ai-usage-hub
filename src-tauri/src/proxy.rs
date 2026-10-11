use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::antigravity;
use crate::models::AntigravityAccount;
use crate::HubState;

static PROXY_RUNNING: AtomicBool = AtomicBool::new(false);
static PROXY_PORT: AtomicU16 = AtomicU16::new(DEFAULT_PROXY_PORT);
static TOTAL_REQUESTS: AtomicU64 = AtomicU64::new(0);
static SUCCESS_REQUESTS: AtomicU64 = AtomicU64::new(0);
static FAILED_REQUESTS: AtomicU64 = AtomicU64::new(0);
static LAST_LATENCY_MS: AtomicU64 = AtomicU64::new(0);

static ACTIVE_ACCOUNT_EMAIL: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
static RECENT_LOGS: LazyLock<Mutex<VecDeque<ProxyRequestLog>>> = LazyLock::new(|| Mutex::new(VecDeque::with_capacity(60)));
static PROXY_HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(90))
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .build()
        .unwrap_or_else(|_| Client::new())
});

/// Default port for the local AI proxy gateway
pub const DEFAULT_PROXY_PORT: u16 = 8046;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRequestLog {
    pub id: String,
    pub timestamp: i64,
    pub client_protocol: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub account_email: String,
    pub latency_ms: u64,
    pub status_code: u16,
    pub is_stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatus {
    pub running: bool,
    pub port: u16,
    pub total_requests: u64,
    pub success_requests: u64,
    pub failed_requests: u64,
    pub last_latency_ms: u64,
    pub active_account_email: Option<String>,
    pub recent_logs: Vec<ProxyRequestLog>,
}

pub fn record_proxy_log(log: ProxyRequestLog) {
    if let Ok(mut logs) = RECENT_LOGS.lock() {
        if logs.len() >= 50 {
            logs.pop_back();
        }
        logs.push_front(log);
    }
}

pub fn clear_proxy_logs() {
    if let Ok(mut logs) = RECENT_LOGS.lock() {
        logs.clear();
    }
}

/// Starts the local AI API Proxy server in a background thread
pub fn start_proxy_server(app_handle: AppHandle, port: u16) {
    if PROXY_RUNNING.swap(true, Ordering::SeqCst) {
        tracing::info!("AI Proxy server is already running");
        return;
    }

    thread::spawn(move || {
        let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(l) => {
                PROXY_PORT.store(port, Ordering::Relaxed);
                tracing::info!(port = port, "AI Proxy Gateway started at http://127.0.0.1:{}", port);
                l
            }
            Err(e) => {
                tracing::warn!(port = port, error = %e, "Could not bind AI Proxy to port, trying next port");
                match TcpListener::bind(format!("127.0.0.1:{}", port + 1)) {
                    Ok(l) => {
                        PROXY_PORT.store(port + 1, Ordering::Relaxed);
                        tracing::info!(port = port + 1, "AI Proxy Gateway started at http://127.0.0.1:{}", port + 1);
                        l
                    }
                    Err(e2) => {
                        tracing::error!(error = %e2, "Failed to start AI Proxy server");
                        PROXY_RUNNING.store(false, Ordering::SeqCst);
                        return;
                    }
                }
            }
        };

        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let handle_clone = app_handle.clone();
                    thread::spawn(move || {
                        let state = handle_clone.state::<HubState>();
                        let _ = handle_proxy_connection(&mut stream, &state);
                    });
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Proxy listener accept error");
                }
            }
        }
        PROXY_RUNNING.store(false, Ordering::SeqCst);
    });
}

fn handle_proxy_connection(stream: &mut TcpStream, state: &HubState) -> Result<(), Box<dyn std::error::Error>> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));

    let (method, path, body_str) = {
        let mut reader = BufReader::new(&mut *stream);
        let mut request_line = String::new();
        if reader.read_line(&mut request_line)? == 0 {
            return Ok(());
        }

        let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
        if parts.len() < 2 {
            return Ok(());
        }

        let method = parts[0].to_string();
        let path = parts[1].to_string();

        // Read headers
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
                break;
            }
            if line.to_ascii_lowercase().starts_with("content-length:") {
                if let Some(val) = line.split(':').nth(1) {
                    content_length = val.trim().parse().unwrap_or(0);
                }
            }
        }

        // Read body if content-length > 0
        let mut body = vec![0u8; content_length];
        if content_length > 0 {
            reader.read_exact(&mut body)?;
        }

        let body_str = String::from_utf8_lossy(&body).to_string();
        (method, path, body_str)
    };

    // Handle CORS preflight
    if method == "OPTIONS" {
        send_http_response(
            stream,
            200,
            "OK",
            "text/plain",
            "",
            &[
                ("Access-Control-Allow-Origin", "*"),
                ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
                ("Access-Control-Allow-Headers", "Content-Type, Authorization, x-api-key, anthropic-version, anthropic-beta"),
            ],
        )?;
        return Ok(());
    }

    let clean_path = path.split('?').next().unwrap_or(&path);

    match (method.as_str(), clean_path) {
        ("GET", "/") | ("GET", "/health") => {
            let res = json!({
                "status": "online",
                "service": "AI Usage Hub AI Proxy Gateway",
                "version": "0.1.17",
                "endpoints": [
                    "/v1/chat/completions",
                    "/v1/messages",
                    "/v1/models",
                    "/v1beta/models"
                ]
            });
            send_json_response(stream, 200, &res)?;
        }
        ("GET", "/v1/models") | ("GET", "/models") | ("GET", "/v1beta/models") => {
            let res = json!({
                "object": "list",
                "data": [
                    {"id": "claude-3-7-sonnet", "object": "model", "owned_by": "anthropic"},
                    {"id": "claude-3-5-sonnet", "object": "model", "owned_by": "anthropic"},
                    {"id": "claude-3-5-haiku", "object": "model", "owned_by": "anthropic"},
                    {"id": "claude-sonnet-4-6", "object": "model", "owned_by": "anthropic"},
                    {"id": "claude-opus-4", "object": "model", "owned_by": "anthropic"},
                    {"id": "gemini-2.5-pro", "object": "model", "owned_by": "google"},
                    {"id": "gemini-2.5-flash", "object": "model", "owned_by": "google"},
                    {"id": "gemini-2.0-flash", "object": "model", "owned_by": "google"},
                    {"id": "gemini-3.6-flash-high", "object": "model", "owned_by": "google"},
                    {"id": "gemini-pro-agent", "object": "model", "owned_by": "google"},
                    {"id": "gpt-4o", "object": "model", "owned_by": "openai"},
                    {"id": "gpt-4o-mini", "object": "model", "owned_by": "openai"},
                    {"id": "gpt-oss-120b-medium", "object": "model", "owned_by": "openai"},
                    {"id": "o1", "object": "model", "owned_by": "openai"},
                    {"id": "o3-mini", "object": "model", "owned_by": "openai"},
                    {"id": "imagen-3", "object": "model", "owned_by": "google"}
                ]
            });
            send_json_response(stream, 200, &res)?;
        }
        ("POST", "/v1/chat/completions") | ("POST", "/chat/completions") => {
            handle_chat_completions(stream, state, &body_str)?;
        }
        ("POST", "/v1/messages") | ("POST", "/messages") => {
            handle_anthropic_messages(stream, state, &body_str)?;
        }
        // Google Gemini Native API Format (lbjlaq feature)
        ("POST", p) if p.contains("/models/") && (p.contains(":generateContent") || p.contains(":streamGenerateContent")) => {
            handle_gemini_native(stream, state, p, &body_str)?;
        }
        _ => {
            let err = json!({"error": {"message": format!("Route not found: {} {}", method, path)}});
            send_json_response(stream, 404, &err)?;
        }
    }

    Ok(())
}

fn send_json_response(stream: &mut TcpStream, status_code: u16, data: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::to_string(data)?;
    send_http_response(
        stream,
        status_code,
        if status_code == 200 { "OK" } else { "Error" },
        "application/json",
        &payload,
        &[("Access-Control-Allow-Origin", "*")],
    )
}

fn send_http_response(
    stream: &mut TcpStream,
    status_code: u16,
    status_text: &str,
    content_type: &str,
    body: &str,
    extra_headers: &[(&str, &str)],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        status_code,
        status_text,
        content_type,
        body.len()
    );
    for (key, val) in extra_headers {
        response.push_str(&format!("{}: {}\r\n", key, val));
    }
    response.push_str("\r\n");
    response.push_str(body);

    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct OpenAIChatRequest {
    model: Option<String>,
    messages: Option<Vec<OpenAIChatMessage>>,
    #[serde(default)]
    temperature: Option<f64>,
    #[serde(default)]
    max_tokens: Option<u64>,
    #[serde(default)]
    max_completion_tokens: Option<u64>,
    #[serde(default)]
    reasoning_effort: Option<String>,
    #[serde(default)]
    stream: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
struct OpenAIChatMessage {
    role: String,
    content: Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessagesRequest {
    model: Option<String>,
    messages: Option<Vec<OpenAIChatMessage>>,
    system: Option<Value>,
    #[serde(default)]
    max_tokens: Option<u64>,
    #[serde(default)]
    thinking: Option<Value>,
    #[serde(default)]
    stream: Option<bool>,
}

/// Map model names to appropriate CloudCode internal upstream model
pub fn resolve_upstream_model(input_model: &str) -> &'static str {
    let lower = input_model.to_ascii_lowercase();
    // Silent background downgrading: Preserves Sonnet/Pro quota on Claude CLI background/title tasks
    if lower.contains("haiku") {
        "gemini-3.6-flash-high"
    } else if lower.contains("opus") || lower.contains("thinking") || lower.contains("o1") || lower.contains("o3") || lower.contains("r1") {
        "claude-opus-4-6-thinking"
    } else if lower.contains("claude") || lower.contains("sonnet") {
        "claude-sonnet-4-6"
    } else if lower.contains("gpt") || lower.contains("oss") {
        "gpt-oss-120b-medium"
    } else if lower.contains("pro") || lower.contains("agent") {
        "gemini-pro-agent"
    } else {
        "gemini-3.6-flash-high"
    }
}

/// Dispatches an upstream generation request with automatic token refresh, failover, and model fallback
fn execute_upstream_request(
    state: &HubState,
    gemini_inner_request: &Value,
    initial_upstream_model: &str,
) -> Result<(Value, String), String> {
    let client = &PROXY_HTTP_CLIENT;

    let max_retries = 3;
    let mut current_model = initial_upstream_model.to_string();

    for attempt in 0..max_retries {
        // Fetch active or best available account from the pool (with automatic token refresh)
        let active_account = get_active_account(state)?;
        let access_token = active_account.access_token.clone();
        let project_id = active_account.project_id.as_deref().unwrap_or("aicode-consumers");
        let account_email = active_account.email.clone();

        let request_id = format!(
            "agent/{:x}/{}/{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            chrono::Utc::now().timestamp_millis(),
            attempt + 1
        );

        // Wrap the payload in Google CloudCode v1internal envelope
        let envelope = json!({
            "project": project_id,
            "requestId": request_id,
            "model": current_model,
            "userAgent": "antigravity",
            "request": gemini_inner_request
        });

        let candidate_urls = [
            "https://daily-cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse",
            "https://cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse",
        ];

        let mut last_err = String::new();
        let mut last_status_code = 0u16;

        for url in &candidate_urls {
            let mut req_builder = client
                .post(*url)
                .bearer_auth(&access_token)
                .header(reqwest::header::USER_AGENT, "antigravity")
                .header(reqwest::header::CONTENT_TYPE, "application/json");

            if current_model.contains("claude") {
                req_builder = req_builder.header("anthropic-beta", "claude-code-20250219");
            }

            match req_builder.json(&envelope).send() {
                Ok(resp) => {
                    let status = resp.status();
                    last_status_code = status.as_u16();
                    let text = resp.text().unwrap_or_default();

                    if status.is_success() {
                        let mut combined_candidates = Vec::new();
                        let mut combined_usage = json!({});

                        for line in text.lines() {
                            let line = line.trim();
                            if let Some(json_str) = line.strip_prefix("data: ") {
                                if json_str == "[DONE]" {
                                    continue;
                                }
                                if let Ok(chunk_val) = serde_json::from_str::<Value>(json_str) {
                                    if let Some(cands) = chunk_val.pointer("/response/candidates").and_then(|v| v.as_array()) {
                                        for c in cands {
                                            combined_candidates.push(c.clone());
                                        }
                                    }
                                    if let Some(usage) = chunk_val.pointer("/response/usageMetadata") {
                                        combined_usage = usage.clone();
                                    }
                                }
                            }
                        }

                        if !combined_candidates.is_empty() {
                            let val = json!({
                                "response": {
                                    "candidates": combined_candidates,
                                    "usageMetadata": combined_usage
                                }
                            });
                            return Ok((val, account_email));
                        }

                        if let Ok(val) = serde_json::from_str::<Value>(&text) {
                            return Ok((val, account_email));
                        }
                    }

                    last_err = format!("Upstream {} ({}): {}", url, status, text);

                    // If rate limited or 403 on this endpoint, break to rotate account
                    if status.as_u16() == 429 || status.as_u16() == 403 {
                        break;
                    }
                }
                Err(e) => {
                    last_err = format!("Network error calling {}: {}", url, e);
                }
            }
        }

        // Handle 401: Token expired mid-flight -> force refresh
        if last_status_code == 401 {
            tracing::warn!(account = %active_account.email, "Access token 401 Unauthorized; refreshing token immediately");
            if let Ok(db) = state.database.lock() {
                if let Ok(Some(mut acc)) = db.antigravity_account(&active_account.id) {
                    if let Ok(token_resp) = antigravity::refresh_google_oauth_token(&acc.refresh_token) {
                        acc.access_token = token_resp.access_token;
                        acc.expiry_timestamp = chrono::Utc::now().timestamp() + token_resp.expires_in;
                        let _ = db.save_antigravity_account(&acc);
                    }
                }
            }
            thread::sleep(Duration::from_millis(200));
            continue;
        }

        // Handle 503 capacity exhausted on gemini-2.5-pro -> fallback to gemini-2.5-flash
        if last_status_code == 503 && current_model == "gemini-2.5-pro" {
            tracing::warn!("gemini-2.5-pro server capacity exhausted (503); falling back to gemini-2.5-flash");
            current_model = "gemini-2.5-flash".into();
            continue;
        }

        // Handle 403 Forbidden: Permission anomaly or suspended project (lbjlaq feature)
        if last_status_code == 403 {
            tracing::warn!(
                attempt = attempt,
                account = %active_account.email,
                "Account encountered 403 Forbidden permission anomaly; marking account forbidden"
            );
            mark_account_status(state, &active_account.id, "forbidden");
            let _ = antigravity::check_and_auto_switch(state);
            thread::sleep(Duration::from_millis(200));
            continue;
        }

        // Handle 429: Rate limited or quota exhausted -> rotate account
        if last_status_code == 429 {
            tracing::warn!(
                attempt = attempt,
                account = %active_account.email,
                status = last_status_code,
                "Account quota exhausted or rate-limited; rotating to next account in pool"
            );
            mark_account_status(state, &active_account.id, "rate_limited");
            let _ = antigravity::check_and_auto_switch(state);
            thread::sleep(Duration::from_millis(200));
            continue;
        }

        if attempt + 1 == max_retries {
            return Err(format!("Upstream error: {}", last_err));
        }
        thread::sleep(Duration::from_millis(500));
    }

    Err("All pool accounts exhausted or rate-limited".into())
}

/// Retrieves an active account from the pool, proactively refreshing expired OAuth tokens
fn get_active_account(state: &HubState) -> Result<AntigravityAccount, String> {
    let db = state.database.lock().map_err(|_| "Database lock error".to_string())?;
    let accounts = db.antigravity_accounts().map_err(|e| e.to_string())?;

    if accounts.is_empty() {
        return Err("No Antigravity accounts registered in AI Usage Hub pool. Please sign in or import an account.".into());
    }

    let settings = db.settings().unwrap_or_default();
    let strategy = settings.antigravity_switch_strategy.as_str();

    // Try current healthy account, then highest healthy quota account (skip rate_limited, expired, and forbidden)
    let healthy: Vec<&AntigravityAccount> = accounts
        .iter()
        .filter(|a| a.status != "rate_limited" && a.status != "expired" && a.status != "forbidden")
        .collect();

    let chosen_opt = if !healthy.is_empty() {
        match strategy {
            "round_robin" => {
                let mut sorted = healthy.clone();
                sorted.sort_by_key(|a| a.last_used);
                sorted.first().cloned().cloned()
            }
            "highest_quota" => {
                let mut sorted = healthy.clone();
                sorted.sort_by(|a, b| {
                    let qa = a.quota_remaining_percent.unwrap_or(100.0);
                    let qb = b.quota_remaining_percent.unwrap_or(100.0);
                    qb.partial_cmp(&qa).unwrap_or(std::cmp::Ordering::Equal)
                });
                sorted.first().cloned().cloned()
            }
            _ => {
                healthy.iter().find(|a| a.is_current).cloned().cloned().or_else(|| {
                    let mut sorted = healthy.clone();
                    sorted.sort_by(|a, b| {
                        let qa = a.quota_remaining_percent.unwrap_or(100.0);
                        let qb = b.quota_remaining_percent.unwrap_or(100.0);
                        qb.partial_cmp(&qa).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    sorted.first().cloned().cloned()
                })
            }
        }
    } else {
        // Fallback: pick the account with highest quota or oldest last_used
        let mut fallback = accounts.clone();
        fallback.sort_by(|a, b| {
            let qa = a.quota_remaining_percent.unwrap_or(0.0);
            let qb = b.quota_remaining_percent.unwrap_or(0.0);
            qb.partial_cmp(&qa).unwrap_or(std::cmp::Ordering::Equal)
        });
        fallback.first().cloned()
    };

    let mut chosen = chosen_opt.ok_or("No suitable account available")?;

    // Check token expiry - if expiring within 60s, refresh immediately!
    let now = chrono::Utc::now().timestamp();
    if chosen.expiry_timestamp <= now + 60 && !chosen.refresh_token.is_empty() {
        tracing::info!(account = %chosen.email, "Refreshing expired Google OAuth access token for proxy request");
        if let Ok(token_resp) = antigravity::refresh_google_oauth_token(&chosen.refresh_token) {
            chosen.access_token = token_resp.access_token;
            chosen.expiry_timestamp = now + token_resp.expires_in;
            if let Some(id_tok) = token_resp.id_token {
                chosen.id_token = Some(id_tok);
            }
            let _ = db.save_antigravity_account(&chosen);
            tracing::info!(account = %chosen.email, "Access token refreshed successfully");
        }
    }

    // Auto-heal rate_limited account if quota is restored
    if chosen.status == "rate_limited" {
        if let Some((quota, details)) = antigravity::fetch_google_quota_summary(&chosen.access_token, chosen.project_id.as_deref()) {
            if quota > 0.0 {
                chosen.status = "healthy".into();
                chosen.quota_remaining_percent = Some(quota);
                chosen.quota_details = Some(details);
                let _ = db.save_antigravity_account(&chosen);
                tracing::info!(account = %chosen.email, quota = quota, "Account auto-healed and restored to healthy status");
            }
        }
    }

    // Update last_used
    chosen.last_used = now;
    let _ = db.save_antigravity_account(&chosen);

    if let Ok(mut lock) = ACTIVE_ACCOUNT_EMAIL.lock() {
        *lock = Some(chosen.email.clone());
    }

    Ok(chosen)
}

fn mark_account_status(state: &HubState, account_id: &str, status: &str) {
    if let Ok(db) = state.database.lock() {
        if let Ok(Some(mut acc)) = db.antigravity_account(account_id) {
            acc.status = status.into();
            let _ = db.save_antigravity_account(&acc);
        }
    }
}

/// Helper to extract clean response text and token metrics from CloudCode response
fn parse_upstream_candidates(upstream_res: &Value) -> (String, u64, u64) {
    let candidates = upstream_res
        .pointer("/response/candidates")
        .or_else(|| upstream_res.pointer("/candidates"))
        .and_then(|v| v.as_array());

    let mut response_text = String::new();
    if let Some(cand_list) = candidates {
        for cand in cand_list {
            if let Some(parts) = cand.pointer("/content/parts").and_then(|v| v.as_array()) {
                // Collect non-thought text parts
                for part in parts {
                    let is_thought = part.get("thought").and_then(|t| t.as_bool()).unwrap_or(false);
                    if !is_thought {
                        if let Some(txt) = part.get("text").and_then(|t| t.as_str()) {
                            response_text.push_str(txt);
                        }
                    }
                }
            }
        }
        // Fallback if all parts had thought flag or empty
        if response_text.is_empty() {
            for cand in cand_list {
                if let Some(parts) = cand.pointer("/content/parts").and_then(|v| v.as_array()) {
                    for part in parts {
                        if let Some(txt) = part.get("text").and_then(|t| t.as_str()) {
                            response_text.push_str(txt);
                        }
                    }
                }
            }
        }
    }

    let prompt_tokens = upstream_res
        .pointer("/response/usageMetadata/promptTokenCount")
        .or_else(|| upstream_res.pointer("/usageMetadata/promptTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let completion_tokens = upstream_res
        .pointer("/response/usageMetadata/candidatesTokenCount")
        .or_else(|| upstream_res.pointer("/usageMetadata/candidatesTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    (response_text, prompt_tokens, completion_tokens)
}

fn handle_chat_completions(stream: &mut TcpStream, state: &HubState, body: &str) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);

    let req: OpenAIChatRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => {
            FAILED_REQUESTS.fetch_add(1, Ordering::Relaxed);
            let err = json!({"error": {"message": format!("Invalid JSON body: {}", e)}});
            return send_json_response(stream, 400, &err);
        }
    };

    let requested_model = req.model.as_deref().unwrap_or("gemini-2.5-flash");
    let upstream_model = resolve_upstream_model(requested_model);

    // Convert OpenAI messages to Gemini contents
    let mut contents = Vec::new();
    let mut system_instruction = None;

    if let Some(messages) = req.messages {
        for msg in messages {
            let text = match msg.content {
                Value::String(s) => s,
                Value::Array(arr) => {
                    let mut combined = String::new();
                    for item in arr {
                        if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                            combined.push_str(t);
                        }
                    }
                    combined
                }
                _ => msg.content.to_string(),
            };

            if msg.role == "system" || msg.role == "developer" {
                system_instruction = Some(json!({
                    "parts": [{ "text": text }]
                }));
            } else {
                let role = if msg.role == "assistant" { "model" } else { "user" };
                contents.push(json!({
                    "role": role,
                    "parts": [{ "text": text }]
                }));
            }
        }
    }

    if contents.is_empty() {
        contents.push(json!({
            "role": "user",
            "parts": [{ "text": "Hello" }]
        }));
    }

    let max_output_tokens = req.max_completion_tokens.or(req.max_tokens).unwrap_or(4096);
    let mut gen_config = json!({
        "temperature": req.temperature.unwrap_or(0.7),
        "maxOutputTokens": max_output_tokens
    });

    if let Some(ref effort) = req.reasoning_effort {
        let budget = match effort.as_str() {
            "low" => 1024,
            "medium" => 4096,
            "high" => 16384,
            _ => 2048,
        };
        gen_config["thinkingConfig"] = json!({ "thinkingBudget": budget });
    }

    let mut gemini_payload = json!({
        "contents": contents,
        "generationConfig": gen_config
    });

    if let Some(sys) = system_instruction {
        gemini_payload["systemInstruction"] = sys;
    }

    match execute_upstream_request(state, &gemini_payload, upstream_model) {
        Ok((upstream_res, account_email)) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            SUCCESS_REQUESTS.fetch_add(1, Ordering::Relaxed);
            LAST_LATENCY_MS.store(latency_ms, Ordering::Relaxed);
            let (response_text, prompt_tokens, completion_tokens) = parse_upstream_candidates(&upstream_res);

            record_proxy_log(ProxyRequestLog {
                id: format!("req_{}", chrono::Utc::now().timestamp_millis()),
                timestamp: chrono::Utc::now().timestamp(),
                client_protocol: "openai".into(),
                requested_model: requested_model.into(),
                upstream_model: upstream_model.into(),
                account_email,
                latency_ms,
                status_code: 200,
                is_stream: req.stream.unwrap_or(false),
            });

            // If streaming was requested, format as SSE chunks
            if req.stream.unwrap_or(false) {
                let chunk_id = format!("chatcmpl-{}", chrono::Utc::now().timestamp_millis());
                let header_bytes = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
                stream.write_all(header_bytes.as_bytes())?;

                let chunk1 = json!({
                    "id": chunk_id,
                    "object": "chat.completion.chunk",
                    "created": chrono::Utc::now().timestamp(),
                    "model": requested_model,
                    "choices": [
                        {
                            "index": 0,
                            "delta": {
                                "role": "assistant",
                                "content": response_text
                            },
                            "finish_reason": Value::Null
                        }
                    ]
                });
                let _ = stream.write_all(format!("data: {}\n\n", chunk1).as_bytes());

                let chunk2 = json!({
                    "id": chunk_id,
                    "object": "chat.completion.chunk",
                    "created": chrono::Utc::now().timestamp(),
                    "model": requested_model,
                    "choices": [
                        {
                            "index": 0,
                            "delta": {},
                            "finish_reason": "stop"
                        }
                    ],
                    "usage": {
                        "prompt_tokens": prompt_tokens,
                        "completion_tokens": completion_tokens,
                        "total_tokens": prompt_tokens + completion_tokens
                    }
                });
                let _ = stream.write_all(format!("data: {}\n\ndata: [DONE]\n\n", chunk2).as_bytes());
                stream.flush()?;
                return Ok(());
            }

            let openai_response = json!({
                "id": format!("chatcmpl-{}", chrono::Utc::now().timestamp_millis()),
                "object": "chat.completion",
                "created": chrono::Utc::now().timestamp(),
                "model": requested_model,
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": response_text
                        },
                        "finish_reason": "stop"
                    }
                ],
                "usage": {
                    "prompt_tokens": prompt_tokens,
                    "completion_tokens": completion_tokens,
                    "total_tokens": prompt_tokens + completion_tokens
                }
            });
            send_json_response(stream, 200, &openai_response)?;
        }
        Err(err_msg) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            FAILED_REQUESTS.fetch_add(1, Ordering::Relaxed);
            LAST_LATENCY_MS.store(latency_ms, Ordering::Relaxed);
            record_proxy_log(ProxyRequestLog {
                id: format!("req_{}", chrono::Utc::now().timestamp_millis()),
                timestamp: chrono::Utc::now().timestamp(),
                client_protocol: "openai".into(),
                requested_model: requested_model.into(),
                upstream_model: upstream_model.into(),
                account_email: "-".into(),
                latency_ms,
                status_code: 502,
                is_stream: req.stream.unwrap_or(false),
            });
            let err = json!({"error": {"message": err_msg}});
            send_json_response(stream, 502, &err)?;
        }
    }

    Ok(())
}

fn handle_anthropic_messages(stream: &mut TcpStream, state: &HubState, body: &str) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);

    let req: AnthropicMessagesRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => {
            FAILED_REQUESTS.fetch_add(1, Ordering::Relaxed);
            let err = json!({"error": {"type": "invalid_request_error", "message": format!("Invalid JSON: {}", e)}});
            return send_json_response(stream, 400, &err);
        }
    };

    let requested_model = req.model.as_deref().unwrap_or("claude-3-7-sonnet");
    let upstream_model = resolve_upstream_model(requested_model);

    let mut contents = Vec::new();
    let mut system_instruction = None;

    if let Some(sys) = req.system {
        let text = match sys {
            Value::String(s) => s,
            Value::Array(arr) => {
                let mut combined = String::new();
                for item in arr {
                    if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                        combined.push_str(t);
                    }
                }
                combined
            }
            _ => sys.to_string(),
        };
        system_instruction = Some(json!({
            "parts": [{ "text": text }]
        }));
    }

    if let Some(messages) = req.messages {
        for msg in messages {
            let text = match msg.content {
                Value::String(s) => s,
                Value::Array(arr) => {
                    let mut combined = String::new();
                    for item in arr {
                        if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                            combined.push_str(t);
                        }
                    }
                    combined
                }
                _ => msg.content.to_string(),
            };
            let role = if msg.role == "assistant" { "model" } else { "user" };
            contents.push(json!({
                "role": role,
                "parts": [{ "text": text }]
            }));
        }
    }

    if contents.is_empty() {
        contents.push(json!({
            "role": "user",
            "parts": [{ "text": "Hello" }]
        }));
    }

    let mut gen_config = json!({
        "maxOutputTokens": req.max_tokens.unwrap_or(4096)
    });

    if let Some(ref th) = req.thinking {
        let budget = th.get("budget_tokens").and_then(|v| v.as_u64()).unwrap_or(2048);
        gen_config["thinkingConfig"] = json!({ "thinkingBudget": budget });
    }

    let mut gemini_payload = json!({
        "contents": contents,
        "generationConfig": gen_config
    });

    if let Some(sys) = system_instruction {
        gemini_payload["systemInstruction"] = sys;
    }

    match execute_upstream_request(state, &gemini_payload, upstream_model) {
        Ok((upstream_res, account_email)) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            SUCCESS_REQUESTS.fetch_add(1, Ordering::Relaxed);
            LAST_LATENCY_MS.store(latency_ms, Ordering::Relaxed);
            let (response_text, input_tokens, output_tokens) = parse_upstream_candidates(&upstream_res);

            record_proxy_log(ProxyRequestLog {
                id: format!("req_{}", chrono::Utc::now().timestamp_millis()),
                timestamp: chrono::Utc::now().timestamp(),
                client_protocol: "anthropic".into(),
                requested_model: requested_model.into(),
                upstream_model: upstream_model.into(),
                account_email,
                latency_ms,
                status_code: 200,
                is_stream: req.stream.unwrap_or(false),
            });

            // If streaming was requested, format as Anthropic SSE chunks
            if req.stream.unwrap_or(false) {
                let msg_id = format!("msg_{}", chrono::Utc::now().timestamp_millis());
                let header_bytes = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
                stream.write_all(header_bytes.as_bytes())?;

                let ev_start = json!({
                    "type": "message_start",
                    "message": {
                        "id": msg_id,
                        "type": "message",
                        "role": "assistant",
                        "content": [],
                        "model": requested_model,
                        "stop_reason": Value::Null,
                        "stop_sequence": Value::Null,
                        "usage": { "input_tokens": input_tokens, "output_tokens": 0 }
                    }
                });
                let _ = stream.write_all(format!("event: message_start\ndata: {}\n\n", ev_start).as_bytes());

                let ev_block_start = json!({
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": { "type": "text", "text": "" }
                });
                let _ = stream.write_all(format!("event: content_block_start\ndata: {}\n\n", ev_block_start).as_bytes());

                let ev_delta = json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": { "type": "text_delta", "text": response_text }
                });
                let _ = stream.write_all(format!("event: content_block_delta\ndata: {}\n\n", ev_delta).as_bytes());

                let ev_block_stop = json!({ "type": "content_block_stop", "index": 0 });
                let _ = stream.write_all(format!("event: content_block_stop\ndata: {}\n\n", ev_block_stop).as_bytes());

                let ev_msg_delta = json!({
                    "type": "message_delta",
                    "delta": { "stop_reason": "end_turn", "stop_sequence": Value::Null },
                    "usage": { "output_tokens": output_tokens }
                });
                let _ = stream.write_all(format!("event: message_delta\ndata: {}\n\n", ev_msg_delta).as_bytes());

                let ev_msg_stop = json!({ "type": "message_stop" });
                let _ = stream.write_all(format!("event: message_stop\ndata: {}\n\n", ev_msg_stop).as_bytes());

                stream.flush()?;
                return Ok(());
            }

            let anthropic_response = json!({
                "id": format!("msg_{}", chrono::Utc::now().timestamp_millis()),
                "type": "message",
                "role": "assistant",
                "model": requested_model,
                "content": [
                    {
                        "type": "text",
                        "text": response_text
                    }
                ],
                "stop_reason": "end_turn",
                "stop_sequence": null,
                "usage": {
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens
                }
            });
            send_json_response(stream, 200, &anthropic_response)?;
        }
        Err(err_msg) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            FAILED_REQUESTS.fetch_add(1, Ordering::Relaxed);
            LAST_LATENCY_MS.store(latency_ms, Ordering::Relaxed);
            record_proxy_log(ProxyRequestLog {
                id: format!("req_{}", chrono::Utc::now().timestamp_millis()),
                timestamp: chrono::Utc::now().timestamp(),
                client_protocol: "anthropic".into(),
                requested_model: requested_model.into(),
                upstream_model: upstream_model.into(),
                account_email: "-".into(),
                latency_ms,
                status_code: 502,
                is_stream: req.stream.unwrap_or(false),
            });
            let err = json!({"type": "error", "error": {"type": "api_error", "message": err_msg}});
            send_json_response(stream, 502, &err)?;
        }
    }

    Ok(())
}

/// Handles native Google Gemini API format requests (e.g. /v1beta/models/:model:generateContent)
fn handle_gemini_native(
    stream: &mut TcpStream,
    state: &HubState,
    path: &str,
    body: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);

    let req_val: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            FAILED_REQUESTS.fetch_add(1, Ordering::Relaxed);
            let err = json!({"error": {"code": 400, "message": format!("Invalid JSON body: {}", e)}});
            return send_json_response(stream, 400, &err);
        }
    };

    // Extract model name from path (e.g. /v1beta/models/gemini-2.5-flash:generateContent)
    let model_part = path.split("/models/").nth(1).unwrap_or("");
    let model_name = model_part.split(':').next().unwrap_or("gemini-2.5-flash");
    let is_stream = path.contains(":streamGenerateContent");
    let upstream_model = resolve_upstream_model(model_name);

    match execute_upstream_request(state, &req_val, upstream_model) {
        Ok((upstream_res, account_email)) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            SUCCESS_REQUESTS.fetch_add(1, Ordering::Relaxed);
            LAST_LATENCY_MS.store(latency_ms, Ordering::Relaxed);

            record_proxy_log(ProxyRequestLog {
                id: format!("req_{}", chrono::Utc::now().timestamp_millis()),
                timestamp: chrono::Utc::now().timestamp(),
                client_protocol: "gemini".into(),
                requested_model: model_name.into(),
                upstream_model: upstream_model.into(),
                account_email,
                latency_ms,
                status_code: 200,
                is_stream,
            });

            let candidates = upstream_res
                .pointer("/response/candidates")
                .or_else(|| upstream_res.pointer("/candidates"))
                .cloned()
                .unwrap_or_else(|| json!([]));
            let usage = upstream_res
                .pointer("/response/usageMetadata")
                .or_else(|| upstream_res.pointer("/usageMetadata"))
                .cloned()
                .unwrap_or_else(|| json!({}));

            let gemini_resp = json!({
                "candidates": candidates,
                "usageMetadata": usage
            });

            if is_stream {
                let header_bytes = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
                stream.write_all(header_bytes.as_bytes())?;
                let chunk_str = format!("data: {}\n\n", serde_json::to_string(&gemini_resp)?);
                stream.write_all(chunk_str.as_bytes())?;
                stream.flush()?;
                return Ok(());
            }

            send_json_response(stream, 200, &gemini_resp)?;
        }
        Err(err_msg) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            FAILED_REQUESTS.fetch_add(1, Ordering::Relaxed);
            LAST_LATENCY_MS.store(latency_ms, Ordering::Relaxed);
            record_proxy_log(ProxyRequestLog {
                id: format!("req_{}", chrono::Utc::now().timestamp_millis()),
                timestamp: chrono::Utc::now().timestamp(),
                client_protocol: "gemini".into(),
                requested_model: model_name.into(),
                upstream_model: upstream_model.into(),
                account_email: "-".into(),
                latency_ms,
                status_code: 502,
                is_stream,
            });
            let err = json!({"error": {"code": 502, "message": err_msg}});
            send_json_response(stream, 502, &err)?;
        }
    }

    Ok(())
}

/// Returns live status and statistics for the local AI proxy gateway
pub fn get_proxy_status(state: &HubState) -> ProxyStatus {
    let active_email = get_active_account(state).ok().map(|a| a.email);
    let logs = RECENT_LOGS
        .lock()
        .map(|l| l.iter().cloned().collect())
        .unwrap_or_default();

    ProxyStatus {
        running: PROXY_RUNNING.load(Ordering::SeqCst),
        port: PROXY_PORT.load(Ordering::Relaxed),
        total_requests: TOTAL_REQUESTS.load(Ordering::Relaxed),
        success_requests: SUCCESS_REQUESTS.load(Ordering::Relaxed),
        failed_requests: FAILED_REQUESTS.load(Ordering::Relaxed),
        last_latency_ms: LAST_LATENCY_MS.load(Ordering::Relaxed),
        active_account_email: active_email,
        recent_logs: logs,
    }
}

/// Runs a direct test prompt through the upstream relay to verify live connectivity
pub fn test_prompt(state: &HubState, prompt: &str, model: &str) -> Result<(String, u64), String> {
    let start = Instant::now();
    TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let upstream_model = resolve_upstream_model(model);
    let payload = json!({
        "contents": [{
            "role": "user",
            "parts": [{ "text": prompt }]
        }],
        "generationConfig": {
            "maxOutputTokens": 256
        }
    });

    match execute_upstream_request(state, &payload, upstream_model) {
        Ok((upstream_res, account_email)) => {
            let (text, _, _) = parse_upstream_candidates(&upstream_res);
            let latency_ms = start.elapsed().as_millis() as u64;
            LAST_LATENCY_MS.store(latency_ms, Ordering::Relaxed);
            SUCCESS_REQUESTS.fetch_add(1, Ordering::Relaxed);

            record_proxy_log(ProxyRequestLog {
                id: format!("req_{}", chrono::Utc::now().timestamp_millis()),
                timestamp: chrono::Utc::now().timestamp(),
                client_protocol: "openai".into(),
                requested_model: model.into(),
                upstream_model: upstream_model.into(),
                account_email,
                latency_ms,
                status_code: 200,
                is_stream: false,
            });

            Ok((text, latency_ms))
        }
        Err(e) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            FAILED_REQUESTS.fetch_add(1, Ordering::Relaxed);
            LAST_LATENCY_MS.store(latency_ms, Ordering::Relaxed);

            record_proxy_log(ProxyRequestLog {
                id: format!("req_{}", chrono::Utc::now().timestamp_millis()),
                timestamp: chrono::Utc::now().timestamp(),
                client_protocol: "openai".into(),
                requested_model: model.into(),
                upstream_model: upstream_model.into(),
                account_email: "-".into(),
                latency_ms,
                status_code: 502,
                is_stream: false,
            });

            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_upstream_model() {
        assert_eq!(resolve_upstream_model("gpt-4o"), "gpt-oss-120b-medium");
        assert_eq!(resolve_upstream_model("claude-3-7-sonnet"), "claude-sonnet-4-6");
        assert_eq!(resolve_upstream_model("claude-opus-4"), "claude-opus-4-6-thinking");
        assert_eq!(resolve_upstream_model("gemini-2.5-pro"), "gemini-pro-agent");
        assert_eq!(resolve_upstream_model("gemini-2.0-flash"), "gemini-3.6-flash-high");
        // lbjlaq feature: Haiku is silently downgraded to Flash to preserve Sonnet quota
        assert_eq!(resolve_upstream_model("claude-3-5-haiku"), "gemini-3.6-flash-high");
        assert_eq!(resolve_upstream_model("claude-haiku"), "gemini-3.6-flash-high");
    }
}
