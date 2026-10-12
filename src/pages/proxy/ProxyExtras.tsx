import { Check, Copy, PlugZap, ShieldAlert, SlidersHorizontal, Terminal } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "../../i18n";
import { api } from "../../lib/ipc";
import { useHub } from "../../stores/hub";
import type { CliConfigStatus } from "../../types";

/** Blocks the page with the account-risk notice until it is accepted; the proxy refuses requests until then. */
export function ProxyRiskGate() {
  const { t } = useTranslation();
  const settings = useHub((state) => state.settings);
  const [dismissed, setDismissed] = useState(false);
  if (!settings || settings.proxyRiskAcknowledged) return null;

  const accept = async () => useHub.setState({ settings: await api.acknowledgeProxyRisk() });

  if (dismissed) {
    return (
      <div className="error-banner" style={{ marginBottom: 18 }}>
        <ShieldAlert size={14} />
        <span>{t("The proxy is off: it answers 403 until you accept the account-risk notice.")}</span>
        <button type="button" className="secondary-button" onClick={() => setDismissed(false)}>{t("Read the notice")}</button>
      </div>
    );
  }
  return (
    <div className="modal-backdrop">
      <div className="glass-panel modal-card" role="alertdialog" aria-labelledby="proxy-risk-title">
        <div className="modal-header">
          <h3 id="proxy-risk-title">{t("Account risk when using the proxy")}</h3>
        </div>
        <div className="modal-body" style={{ fontSize: 13, lineHeight: 1.55, color: "var(--text)" }}>
          <p style={{ margin: 0 }}>{t("Switching accounts in the IDE does not affect your Google account. Sending other tools' requests through these accounts does: it is against Google's terms of service and Google has been suspending accounts for it.")}</p>
          <p style={{ margin: 0, fontWeight: 700 }}>{t("Only use accounts you can afford to lose.")}</p>
        </div>
        <div className="modal-actions">
          <button type="button" className="secondary-button" onClick={() => setDismissed(true)}>{t("Keep the proxy off")}</button>
          <button type="button" className="primary-button" onClick={() => void accept()}>{t("I understand, turn the proxy on")}</button>
        </div>
      </div>
    </div>
  );
}

/** Port, upstream timeout and the background-task saving; rendered inside the proxy settings panel. */
export function ProxyAdvancedSettings() {
  const { t } = useTranslation();
  const { settings, saveSettings } = useHub();
  const [port, setPort] = useState(settings?.proxyPort ?? 8046);
  const [timeoutSeconds, setTimeoutSeconds] = useState(settings?.proxyTimeoutSeconds ?? 90);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setPort(settings?.proxyPort ?? 8046);
    setTimeoutSeconds(settings?.proxyTimeoutSeconds ?? 90);
  }, [settings]);
  if (!settings) return null;

  async function save(change: Partial<typeof settings>) {
    try {
      setError(null);
      await saveSettings({ ...settings!, ...change });
    } catch (reason) {
      setError(String(reason));
    }
  }
  const changed = port !== settings.proxyPort || timeoutSeconds !== settings.proxyTimeoutSeconds;

  return (
    <>
      <div className="settings-section">
        <div>
          <SlidersHorizontal size={19} />
          <span>
            <strong>{t("Port and timeout")}</strong>
            <small>{t("A new port is used after AI Usage Hub restarts. The timeout is how long a request may wait for the model, 10 to 600 seconds.")}</small>
            {error && <small style={{ color: "#f87171" }}>{error}</small>}
          </span>
        </div>
        <div className="proxy-key-row">
          <input className="search-input" type="number" min={1024} max={65535} value={port} onChange={(event) => setPort(Number(event.target.value))} aria-label={t("Proxy port")} style={{ width: 96 }} />
          <input className="search-input" type="number" min={10} max={600} value={timeoutSeconds} onChange={(event) => setTimeoutSeconds(Number(event.target.value))} aria-label={t("Timeout in seconds")} style={{ width: 80 }} />
          <button type="button" className="secondary-button" disabled={!changed} onClick={() => void save({ proxyPort: port, proxyTimeoutSeconds: timeoutSeconds })}>{t("Save")}</button>
        </div>
      </div>
      <div className="settings-section">
        <div>
          <span>
            <strong>{t("Send background tasks to Flash")}</strong>
            <small>{t("Conversation titles and next-prompt suggestions from coding tools go to a Flash model, so they do not spend Claude or Pro quota.")}</small>
          </span>
        </div>
        <label className="switch">
          <input type="checkbox" checked={settings.proxyBackgroundDowngrade} onChange={(event) => void save({ proxyBackgroundDowngrade: event.target.checked })} />
          <span />
        </label>
      </div>
    </>
  );
}

const snippetStyle = { background: "rgba(0,0,0,0.35)", padding: "10px 12px", borderRadius: 8, fontSize: 11, fontFamily: "monospace", color: "#e2e8f0", overflowX: "auto" as const, maxHeight: 150, border: "1px solid rgba(255,255,255,0.08)" };

/** Ready-to-run cURL and Python examples for the OpenAI-style endpoint. */
export function CodeSnippetCard({ baseUrl, apiKey, model }: { baseUrl: string; apiKey: string; model: string }) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<"curl" | "python">("curl");
  const [copied, setCopied] = useState(false);
  const snippets = {
    curl: `curl ${baseUrl}/v1/chat/completions \\\n  -H "Authorization: Bearer ${apiKey}" \\\n  -H "Content-Type: application/json" \\\n  -d '{"model": "${model}", "messages": [{"role": "user", "content": "Hello"}]}'`,
    python: `from openai import OpenAI\n\nclient = OpenAI(base_url="${baseUrl}/v1", api_key="${apiKey}")\nreply = client.chat.completions.create(\n    model="${model}",\n    messages=[{"role": "user", "content": "Hello"}],\n)\nprint(reply.choices[0].message.content)`,
  };

  async function copy() {
    if (!navigator.clipboard) return;
    await navigator.clipboard.writeText(snippets[tab]);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <article className="glass-panel integration-card">
      <header>
        <span className="provider-logo provider-logo--openai"><Terminal size={15} /></span>
        <div>
          <small>cURL / Python</small>
          <strong>{t("Example request")}</strong>
          <span>/v1/chat/completions</span>
        </div>
      </header>
      <div className="segment-group">
        <button type="button" className={`segment-btn ${tab === "curl" ? "is-active" : ""}`} onClick={() => setTab("curl")}>cURL</button>
        <button type="button" className={`segment-btn ${tab === "python" ? "is-active" : ""}`} onClick={() => setTab("python")}>Python</button>
      </div>
      <div style={{ position: "relative" }}>
        <pre style={snippetStyle}>{snippets[tab]}</pre>
        <button type="button" className="icon-button" style={{ position: "absolute", top: 8, right: 8 }} onClick={() => void copy()} title={t("Copy configuration")}>
          {copied ? <Check size={14} color="#dba8c2" /> : <Copy size={14} />}
        </button>
      </div>
      <p className="integration-setup" style={{ fontSize: 12, marginTop: 8 }}>{t("The Python example uses the official openai package.")}</p>
    </article>
  );
}

const tools: { id: keyof CliConfigStatus; name: string; file: string }[] = [
  { id: "claude", name: "Claude Code", file: "~/.claude/settings.json" },
  { id: "opencode", name: "OpenCode", file: "~/.config/opencode/opencode.json" },
];

/** Writes the proxy address and key into Claude Code's or OpenCode's own config file, and takes them out again. */
export function QuickConnectCard() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<CliConfigStatus>({ claude: false, opencode: false });
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api.cliConfigStatus().then(setStatus).catch(() => {});
  }, []);

  async function toggle(tool: keyof CliConfigStatus, name: string, file: string) {
    try {
      setError(null);
      if (status[tool]) {
        setStatus(await api.removeCliConfig(tool));
      } else if (window.confirm(t("Write the proxy address and key into {file}? {name} will then send every request through AI Usage Hub. The original file is kept next to it as .hub-backup.", { file, name }))) {
        setStatus(await api.applyCliConfig(tool));
      }
    } catch (reason) {
      setError(String(reason));
    }
  }

  return (
    <article className="glass-panel integration-card">
      <header>
        <span className="provider-logo provider-logo--anthropic"><PlugZap size={15} /></span>
        <div>
          <small>{t("One-click setup")}</small>
          <strong>Claude Code / OpenCode</strong>
          <span>{t("Edits the tool's own config file")}</span>
        </div>
      </header>
      <div className="integration-apps">
        {tools.map((tool) => (
          <div key={tool.id}>
            <span className={`state-dot state-dot--${status[tool.id] ? "running" : "not_running"}`} />
            <p>
              <strong>{tool.name}</strong>
              <small>{status[tool.id] ? t("Connected to the proxy") : tool.file}</small>
            </p>
            <button type="button" className="secondary-button" onClick={() => void toggle(tool.id, tool.name, tool.file)}>
              {t(status[tool.id] ? "Disconnect" : "Connect")}
            </button>
          </div>
        ))}
      </div>
      {error && <div className="error-banner" style={{ margin: "0 18px 12px" }}><ShieldAlert size={14} /><span>{error}</span></div>}
      <p className="integration-setup" style={{ fontSize: 12 }}>{t("Restart the tool after connecting or disconnecting. Disconnect removes only the entries added here.")}</p>
    </article>
  );
}
