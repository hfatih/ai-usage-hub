//! Owned, bounded connection to Codex's native account protocol. Credentials stay in Codex.
use crate::error::{HubError, Result};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

pub(crate) const UPGRADE: &str =
    "Update Codex to use saved-account switching, then refresh this list.";
const TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) struct Client {
    child: Child,
    stdin: ChildStdin,
    receiver: Receiver<Value>,
    notifications: VecDeque<Value>,
    next_id: u64,
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Client {
    #[cfg(test)]
    pub(crate) fn discover() -> Result<Self> {
        Self::discover_profile(None)
    }

    pub(crate) fn discover_profile(profile: Option<&Path>) -> Result<Self> {
        for executable in crate::providers::openai::codex_candidates() {
            if let Ok(client) = Self::start(&executable, profile) {
                return Ok(client);
            }
        }
        Err(HubError::Operation(
            "Codex could not be started. Install or update Codex, then refresh this list.".into(),
        ))
    }

    fn start(executable: &Path, profile: Option<&Path>) -> Result<Self> {
        let mut command = Command::new(executable);
        crate::account_profiles::configure(
            &mut command,
            crate::account_profiles::Provider::Codex,
            profile,
        );
        command
            .args(["app-server", "--listen", "stdio://"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn()?;
        // These pipes were explicitly requested; clean up even if a platform fails to supply one.
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(HubError::Operation("Codex input is unavailable.".into()));
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(HubError::Operation("Codex output is unavailable.".into()));
        };
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(|line| line.ok()) {
                if let Ok(message) = serde_json::from_str::<Value>(&line) {
                    if sender.send(message).is_err() {
                        break;
                    }
                }
            }
        });
        let mut client = Self {
            child,
            stdin,
            receiver,
            notifications: VecDeque::new(),
            next_id: 1,
        };
        client.request("initialize", json!({
            "clientInfo":{"name":"ai-usage-hub","title":"AI Usage Hub","version":env!("CARGO_PKG_VERSION")},
            "capabilities":{"experimentalApi":true}
        }))?;
        client.send(json!({"method":"initialized"}))?;
        Ok(client)
    }

    fn send(&mut self, message: Value) -> Result<()> {
        serde_json::to_writer(&mut self.stdin, &message)
            .map_err(|_| HubError::Operation("Could not send a Codex request.".into()))?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        Ok(())
    }

    pub(crate) fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"id":id,"method":method,"params":params}))?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let message = self
                .receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| {
                    HubError::Operation(
                        "Codex did not respond in time. Refresh to check the active account."
                            .into(),
                    )
                })?;
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                return rpc_result(message);
            }
            self.notifications.push_back(message);
        }
    }

    pub(crate) fn notification(&mut self) -> Option<Value> {
        self.notifications
            .pop_front()
            .or_else(|| self.receiver.try_recv().ok())
    }
}

fn rpc_result(message: Value) -> Result<Value> {
    if let Some(error) = message.get("error") {
        let message = if error["code"].as_i64() == Some(-32601) {
            UPGRADE
        } else {
            "Codex rejected the account operation. Sign in again or check your workspace permissions."
        };
        // Server errors may include auth diagnostics; never forward their raw payload to the UI.
        return Err(HubError::Operation(message.into()));
    }
    message
        .get("result")
        .cloned()
        .ok_or_else(|| HubError::Operation("Codex returned an invalid account response.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_protocol_has_upgrade_guidance_without_leaking_server_details() {
        let error = rpc_result(json!({"error":{"code":-32601,"message":"secret"}}))
            .unwrap_err()
            .to_string();
        assert!(error.contains("Update Codex"));
        assert!(!error.contains("secret"));
    }
}
