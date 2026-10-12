import {
  Activity,
  ArrowRight,
  Check,
  CheckCircle2,
  Clock3,
  Copy,
  Cpu,
  Eye,
  EyeOff,
  Flame,
  KeyRound,
  Plus,
  Radio,
  RefreshCw,
  Send,
  Server,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  Terminal,
  Trash2,
  Zap,
} from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { useTranslation } from "../i18n";
import { useHub } from "../stores/hub";
import type { ModelMapRule, ProxyModel, ProxyStatus } from "../types";
import { byPlatform } from "../lib/platform";

export function ProxyPage() {
  const { t } = useTranslation();
  const { antigravityAccounts, settings, saveSettings } = useHub();
  const [showKey, setShowKey] = useState(false);
  const [rules, setRules] = useState<ModelMapRule[]>(settings?.proxyModelMap ?? []);
  useEffect(() => setRules(settings?.proxyModelMap ?? []), [settings]);
  const keyRequired = settings?.proxyRequireKey !== false;
  const apiKey = (keyRequired && settings?.proxyApiKey) || "antigravity";

  async function regenerateKey() {
    if (!window.confirm(t("Create a new key? Tools using the old key stop working until you update them."))) return;
    useHub.setState({ settings: await api.regenerateProxyApiKey() });
  }

  function updateRule(index: number, change: Partial<ModelMapRule>) {
    setRules(rules.map((rule, position) => (position === index ? { ...rule, ...change } : rule)));
  }

  const [status, setStatus] = useState<ProxyStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  // Test prompt playground
  const [testModel, setTestModel] = useState("gemini-2.5-flash");
  const [testPrompt, setTestPrompt] = useState("Hello! Tell me a one-sentence programming tip.");
  const [testLoading, setTestLoading] = useState(false);
  const [testResult, setTestResult] = useState<{ text: string; latencyMs: number } | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const [liveModels, setLiveModels] = useState<ProxyModel[]>([]);

  // CLI snippet tab
  const [cliTab, setCliTab] = useState<"bash" | "powershell">("bash");
  const [aiderTab, setAiderTab] = useState<"bash" | "powershell">("bash");

  useEffect(() => {
    void fetchStatus();
    api.proxyModels().then(setLiveModels).catch(() => {});
    const timer = setInterval(() => void fetchStatus(), 5000);
    return () => clearInterval(timer);
  }, []);

  async function fetchStatus() {
    try {
      const s = await api.proxyStatus();
      setStatus(s);
    } catch {
      // Ignored in preview
    }
  }

  async function copyText(key: string, text: string) {
    if (navigator.clipboard) {
      await navigator.clipboard.writeText(text);
      setCopiedKey(key);
      setTimeout(() => setCopiedKey(null), 2000);
    }
  }

  async function handleSendTest() {
    if (!testPrompt.trim()) return;
    try {
      setTestLoading(true);
      setTestError(null);
      setTestResult(null);
      const res = await api.testProxyPrompt(testPrompt.trim(), testModel);
      setTestResult(res);
      await fetchStatus();
    } catch (err) {
      setTestError(String(err));
    } finally {
      setTestLoading(false);
    }
  }

  const port = status?.port ?? 8046;
  const baseUrl = `http://127.0.0.1:${port}`;
  const total = status?.totalRequests ?? 0;
  const success = status?.successRequests ?? 0;
  const successRate = total > 0 ? Math.round((success / total) * 100) : 100;
  const activeEmail = status?.activeAccountEmail || antigravityAccounts.find((a) => a.isCurrent)?.email || antigravityAccounts[0]?.email;

  const claudeBashSnippet = `export ANTHROPIC_BASE_URL="${baseUrl}"\nexport ANTHROPIC_API_KEY="${apiKey}"\nclaude`;
  const claudePsSnippet = `$env:ANTHROPIC_BASE_URL="${baseUrl}"\n$env:ANTHROPIC_API_KEY="${apiKey}"\nclaude`;

  const aiderBashSnippet = `export OPENAI_API_BASE="${baseUrl}/v1"\nexport OPENAI_API_KEY="${apiKey}"\naider --model openai/claude-3-7-sonnet`;
  const aiderPsSnippet = `$env:OPENAI_API_BASE="${baseUrl}/v1"\n$env:OPENAI_API_KEY="${apiKey}"\naider --model openai/claude-3-7-sonnet`;

  const continueSnippet = JSON.stringify(
    {
      models: [
        {
          title: "Antigravity Claude 3.7",
          provider: "openai",
          model: "claude-3-7-sonnet",
          apiBase: `${baseUrl}/v1`,
          apiKey,
        },
      ],
    },
    null,
    2
  );

  const clineSnippet = JSON.stringify(
    {
      "cline.apiProvider": "openai-native",
      "cline.openAiBaseUrl": `${baseUrl}/v1`,
      "cline.openAiApiKey": apiKey,
      "cline.openAiModelId": "claude-3-7-sonnet",
    },
    null,
    2
  );

  return (
    <section>
      <div className="page-heading">
        <div>
          <span className="eyebrow">{t("Local Gateway")}</span>
          <h1>{t("AI API Proxy")}</h1>
          <p>{t("Local OpenAI, Anthropic, and Gemini compatible proxy relay with automatic multi-account rotation.")}</p>
        </div>
        <button
          className="secondary-button"
          onClick={() => {
            setLoading(true);
            void fetchStatus().finally(() => setLoading(false));
          }}
          disabled={loading}
        >
          <RefreshCw size={14} className={loading ? "spin" : ""} />
          {t("Refresh status")}
        </button>
      </div>

      {/* STATUS OVERVIEW STRIP */}
      <div className="summary-strip" style={{ marginBottom: 24 }}>
        <div>
          <span className="summary-icon summary-icon--success">
            <Radio size={18} />
          </span>
          <p>
            <strong>{baseUrl}</strong>
            <small>{status?.running ? t("Proxy running") : t("Starting...")}</small>
          </p>
        </div>

        <div>
          <span className="summary-icon">
            <Cpu size={18} />
          </span>
          <p>
            <strong>{activeEmail ? activeEmail.split("@")[0] : t("None")}</strong>
            <small>{activeEmail ? t("Active relay account") : t("No account in pool")}</small>
          </p>
        </div>

        <div>
          <span className="summary-icon summary-icon--purple">
            <Activity size={18} />
          </span>
          <p>
            <strong>{total} {t("requests")}</strong>
            <small>{successRate}% {t("success rate")}</small>
          </p>
        </div>

        <div className="summary-refresh">
          <small>{t("Last latency")}</small>
          <strong>{status?.lastLatencyMs ? `${status.lastLatencyMs} ms` : "—"}</strong>
        </div>
      </div>

      {/* ACCESS KEY & MODEL RULES */}
      <div className="settings-panel glass-panel" style={{ marginBottom: 28 }}>
        <div className="settings-section">
          <div>
            <KeyRound size={19} />
            <span>
              <strong>{t("API key")}</strong>
              <small>{t(byPlatform("Tools must send this key. Without it, any program on this PC could spend your accounts' quota.", "Tools must send this key. Without it, any program on this Mac could spend your accounts' quota."))}</small>
            </span>
          </div>
          <div className="proxy-key-row">
            <code>{showKey ? settings?.proxyApiKey : "•".repeat(24)}</code>
            <button type="button" className="icon-button" onClick={() => setShowKey(!showKey)} title={t(showKey ? "Hide key" : "Show key")}>
              {showKey ? <EyeOff size={14} /> : <Eye size={14} />}
            </button>
            <button type="button" className="icon-button" onClick={() => void copyText("proxy-key", settings?.proxyApiKey ?? "")} title={t("Copy key")}>
              {copiedKey === "proxy-key" ? <Check size={14} /> : <Copy size={14} />}
            </button>
            <button type="button" className="secondary-button" onClick={() => void regenerateKey()}>
              <RefreshCw size={13} />
              {t("New key")}
            </button>
          </div>
        </div>
        <div className="settings-section">
          <div>
            <span>
              <strong>{t("Require the key")}</strong>
              <small>{t("Turn off only for a tool that cannot send a key.")}</small>
            </span>
          </div>
          <label className="switch">
            <input type="checkbox" checked={keyRequired} onChange={(event) => settings && void saveSettings({ ...settings, proxyRequireKey: event.target.checked })} />
            <span />
          </label>
        </div>
        <div className="settings-section proxy-rules">
          <div>
            <span>
              <strong>{t("Model rules")}</strong>
              <small>{t("Send a model name your tool asks for to the model you choose. End a name with * to match everything that starts with it.")}</small>
            </span>
          </div>
          <div className="proxy-rule-list">
            {rules.map((rule, index) => (
              <div className="proxy-rule" key={index}>
                <input className="search-input" value={rule.from} placeholder="gpt-4*" onChange={(event) => updateRule(index, { from: event.target.value })} />
                <span>→</span>
                {liveModels.length > 0 ? (
                  <select value={rule.to} onChange={(event) => updateRule(index, { to: event.target.value })}>
                    <option value="">{t("Choose a model")}</option>
                    {!liveModels.some((model) => model.id === rule.to) && rule.to && <option value={rule.to}>{rule.to}</option>}
                    {liveModels.map((model) => <option key={model.id} value={model.id}>{model.displayName ?? model.id}</option>)}
                  </select>
                ) : (
                  <input className="search-input" value={rule.to} placeholder="gemini-3.6-flash-high" onChange={(event) => updateRule(index, { to: event.target.value })} />
                )}
                <button type="button" className="icon-button" onClick={() => setRules(rules.filter((_, position) => position !== index))} title={t("Delete")}>
                  <Trash2 size={14} />
                </button>
              </div>
            ))}
            <div className="proxy-rule-actions">
              <button type="button" className="secondary-button" onClick={() => setRules([...rules, { from: "", to: "" }])}>
                <Plus size={13} />
                {t("Add rule")}
              </button>
              <button type="button" className="primary-button" onClick={() => settings && void saveSettings({ ...settings, proxyModelMap: rules })}>
                {t("Save rules")}
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* CLIENT CONFIGURATIONS GRID */}
      <div className="section-heading">
        <div>
          <span className="eyebrow">{t("Client Quick Setup")}</span>
          <h2>{t("Connect your favorite AI tools")}</h2>
        </div>
      </div>

      <div className="integration-grid" style={{ marginBottom: 32 }}>
        {/* CLAUDE CODE CLI */}
        <article className="glass-panel integration-card">
          <header>
            <span className="provider-logo provider-logo--anthropic">A</span>
            <div>
              <small>Claude Code CLI</small>
              <strong>Anthropic Native (/v1/messages)</strong>
              <span>{t("Chain-of-thought & Silent haiku downgrade")}</span>
            </div>
          </header>
          <div className="segment-group">
            <button
              type="button"
              className={`segment-btn ${cliTab === "bash" ? "is-active" : ""}`}
              onClick={() => setCliTab("bash")}
            >
              Bash / Zsh
            </button>
            <button
              type="button"
              className={`segment-btn ${cliTab === "powershell" ? "is-active" : ""}`}
              onClick={() => setCliTab("powershell")}
            >
              PowerShell
            </button>
          </div>
          <div style={{ position: "relative" }}>
            <pre
              style={{
                background: "rgba(0,0,0,0.35)",
                padding: "10px 12px",
                borderRadius: 8,
                fontSize: 12,
                fontFamily: "monospace",
                color: "#e2e8f0",
                overflowX: "auto",
                border: "1px solid rgba(255,255,255,0.08)",
              }}
            >
              {cliTab === "bash" ? claudeBashSnippet : claudePsSnippet}
            </pre>
            <button
              type="button"
              className="icon-button"
              style={{ position: "absolute", top: 8, right: 8 }}
              onClick={() => void copyText("claude", cliTab === "bash" ? claudeBashSnippet : claudePsSnippet)}
              title={t("Copy configuration")}
            >
              {copiedKey === "claude" ? <Check size={14} color="#dba8c2" /> : <Copy size={14} />}
            </button>
          </div>
          <p className="integration-setup" style={{ fontSize: 12, marginTop: 8 }}>
            <ShieldCheck size={12} style={{ display: "inline", verticalAlign: "middle", marginRight: 4 }} />
            {t("Silent Downgrading preserves Sonnet quota by routing background tasks to Flash.")}
          </p>
        </article>

        {/* OPENAI COMPATIBLE CLIENTS */}
        <article className="glass-panel integration-card">
          <header>
            <span className="provider-logo provider-logo--openai">O</span>
            <div>
              <small>OpenAI Format</small>
              <strong>Cursor / Roo Code / NextChat / Cherry Studio</strong>
              <span>/v1/chat/completions</span>
            </div>
          </header>
          <div style={{ display: "flex", flexDirection: "column", gap: 8, margin: "14px 18px" }}>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", fontSize: 12 }}>
              <span style={{ color: "var(--muted)" }}>Base URL:</span>
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <code style={{ background: "rgba(0,0,0,0.3)", padding: "2px 6px", borderRadius: 4 }}>{baseUrl}/v1</code>
                <button
                  type="button"
                  className="icon-button"
                  style={{ padding: 4 }}
                  onClick={() => void copyText("openai-url", `${baseUrl}/v1`)}
                >
                  {copiedKey === "openai-url" ? <Check size={12} color="#dba8c2" /> : <Copy size={12} />}
                </button>
              </div>
            </div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", fontSize: 12 }}>
              <span style={{ color: "var(--muted)" }}>API Key:</span>
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <code style={{ background: "rgba(0,0,0,0.3)", padding: "2px 6px", borderRadius: 4 }}>{apiKey}</code>
                <button
                  type="button"
                  className="icon-button"
                  style={{ padding: 4 }}
                  onClick={() => void copyText("openai-key", apiKey)}
                >
                  {copiedKey === "openai-key" ? <Check size={12} color="#dba8c2" /> : <Copy size={12} />}
                </button>
              </div>
            </div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", fontSize: 12 }}>
              <span style={{ color: "var(--muted)" }}>Models:</span>
              <span style={{ fontSize: 11, color: "var(--fg)" }}>claude-3-7-sonnet, gemini-2.5-pro, gpt-4o</span>
            </div>
          </div>
          <p className="integration-setup" style={{ fontSize: 12 }}>
            <Zap size={12} style={{ display: "inline", verticalAlign: "middle", marginRight: 4 }} />
            {t("Supports streaming, multi-turn chat, and automatic 429 rotation.")}
          </p>
        </article>

        {/* AIDER CLI */}
        <article className="glass-panel integration-card">
          <header>
            <span className="provider-logo provider-logo--openai" style={{ background: "rgba(168, 85, 247, 0.2)", color: "#c084fc" }}>AI</span>
            <div>
              <small>Aider CLI</small>
              <strong>Terminal Pair Programming</strong>
              <span>openai/claude-3-7-sonnet</span>
            </div>
          </header>
          <div className="segment-group">
            <button
              type="button"
              className={`segment-btn ${aiderTab === "bash" ? "is-active" : ""}`}
              onClick={() => setAiderTab("bash")}
            >
              Bash / Zsh
            </button>
            <button
              type="button"
              className={`segment-btn ${aiderTab === "powershell" ? "is-active" : ""}`}
              onClick={() => setAiderTab("powershell")}
            >
              PowerShell
            </button>
          </div>
          <div style={{ position: "relative" }}>
            <pre
              style={{
                background: "rgba(0,0,0,0.35)",
                padding: "10px 12px",
                borderRadius: 8,
                fontSize: 12,
                fontFamily: "monospace",
                color: "#e2e8f0",
                overflowX: "auto",
                border: "1px solid rgba(255,255,255,0.08)",
              }}
            >
              {aiderTab === "bash" ? aiderBashSnippet : aiderPsSnippet}
            </pre>
            <button
              type="button"
              className="icon-button"
              style={{ position: "absolute", top: 8, right: 8 }}
              onClick={() => void copyText("aider", aiderTab === "bash" ? aiderBashSnippet : aiderPsSnippet)}
              title={t("Copy configuration")}
            >
              {copiedKey === "aider" ? <Check size={14} color="#dba8c2" /> : <Copy size={14} />}
            </button>
          </div>
          <p className="integration-setup" style={{ fontSize: 12, marginTop: 8 }}>
            <Zap size={12} style={{ display: "inline", verticalAlign: "middle", marginRight: 4 }} />
            {t("Git repo-aware AI editing with automatic commit generation.")}
          </p>
        </article>

        {/* CONTINUE.DEV */}
        <article className="glass-panel integration-card">
          <header>
            <span className="provider-logo provider-logo--openai" style={{ background: "rgba(59, 130, 246, 0.2)", color: "#60a5fa" }}>C</span>
            <div>
              <small>Continue.dev</small>
              <strong>~/.continue/config.json</strong>
              <span>VS Code & JetBrains</span>
            </div>
          </header>
          <div style={{ position: "relative", margin: "14px 18px 6px" }}>
            <pre
              style={{
                background: "rgba(0,0,0,0.35)",
                padding: "10px 12px",
                borderRadius: 8,
                fontSize: 11,
                fontFamily: "monospace",
                color: "#e2e8f0",
                overflowX: "auto",
                maxHeight: 115,
                border: "1px solid rgba(255,255,255,0.08)",
              }}
            >
              {continueSnippet}
            </pre>
            <button
              type="button"
              className="icon-button"
              style={{ position: "absolute", top: 8, right: 8 }}
              onClick={() => void copyText("continue", continueSnippet)}
              title={t("Copy configuration")}
            >
              {copiedKey === "continue" ? <Check size={14} color="#dba8c2" /> : <Copy size={14} />}
            </button>
          </div>
          <p className="integration-setup" style={{ fontSize: 12 }}>
            <Sparkles size={12} style={{ display: "inline", verticalAlign: "middle", marginRight: 4 }} />
            {t("Open-source AI code assistant for editor sidebars.")}
          </p>
        </article>

        {/* CLINE & ROO CODE */}
        <article className="glass-panel integration-card">
          <header>
            <span className="provider-logo provider-logo--anthropic" style={{ background: "rgba(245, 158, 11, 0.2)", color: "#fbbf24" }}>R</span>
            <div>
              <small>Cline & Roo Code</small>
              <strong>Autonomous Coding Agent</strong>
              <span>VS Code settings.json</span>
            </div>
          </header>
          <div style={{ position: "relative", margin: "14px 18px 6px" }}>
            <pre
              style={{
                background: "rgba(0,0,0,0.35)",
                padding: "10px 12px",
                borderRadius: 8,
                fontSize: 11,
                fontFamily: "monospace",
                color: "#e2e8f0",
                overflowX: "auto",
                maxHeight: 115,
                border: "1px solid rgba(255,255,255,0.08)",
              }}
            >
              {clineSnippet}
            </pre>
            <button
              type="button"
              className="icon-button"
              style={{ position: "absolute", top: 8, right: 8 }}
              onClick={() => void copyText("cline", clineSnippet)}
              title={t("Copy configuration")}
            >
              {copiedKey === "cline" ? <Check size={14} color="#dba8c2" /> : <Copy size={14} />}
            </button>
          </div>
          <p className="integration-setup" style={{ fontSize: 12 }}>
            <ShieldCheck size={12} style={{ display: "inline", verticalAlign: "middle", marginRight: 4 }} />
            {t("Full MCP tool calling and autonomous workspace execution.")}
          </p>
        </article>

        {/* GOOGLE GEMINI NATIVE */}
        <article className="glass-panel integration-card">
          <header>
            <span className="provider-logo provider-logo--google">G</span>
            <div>
              <small>Google GenAI Native</small>
              <strong>Official @google/genai & Python SDKs</strong>
              <span>/v1beta/models</span>
            </div>
          </header>
          <div style={{ display: "flex", flexDirection: "column", gap: 8, margin: "14px 18px" }}>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", fontSize: 12 }}>
              <span style={{ color: "var(--muted)" }}>Base URL:</span>
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <code style={{ background: "rgba(0,0,0,0.3)", padding: "2px 6px", borderRadius: 4 }}>{baseUrl}/v1beta</code>
                <button
                  type="button"
                  className="icon-button"
                  style={{ padding: 4 }}
                  onClick={() => void copyText("gemini-url", `${baseUrl}/v1beta`)}
                >
                  {copiedKey === "gemini-url" ? <Check size={12} color="#dba8c2" /> : <Copy size={12} />}
                </button>
              </div>
            </div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", fontSize: 12 }}>
              <span style={{ color: "var(--muted)" }}>Endpoints:</span>
              <span style={{ fontSize: 11, color: "var(--fg)" }}>:generateContent, :streamGenerateContent</span>
            </div>
          </div>
          <p className="integration-setup" style={{ fontSize: 12 }}>
            <Sparkles size={12} style={{ display: "inline", verticalAlign: "middle", marginRight: 4 }} />
            {t("100% native Gemini payload format without conversion loss.")}
          </p>
        </article>
      </div>

      {/* LIVE TEST SANDBOX */}
      <div className="section-heading">
        <div>
          <span className="eyebrow">{t("Sandbox & Verification")}</span>
          <h2>{t("Test Proxy Relay")}</h2>
        </div>
      </div>

      <div className="glass-panel" style={{ padding: 20, marginBottom: 32 }}>
        <div style={{ display: "flex", gap: 12, alignItems: "center", marginBottom: 12 }}>
          <select
            value={testModel}
            onChange={(e) => setTestModel(e.target.value)}
            style={{ width: "auto", minWidth: 200 }}
          >
            {liveModels.length > 0 ? (
              liveModels.map((model) => (
                <option key={model.id} value={model.id}>
                  {model.displayName ?? model.id}
                  {model.remainingPercent != null ? ` · ${Math.round(model.remainingPercent)}%` : ""}
                </option>
              ))
            ) : (
              <>
                <option value="gemini-2.5-flash">gemini-2.5-flash</option>
                <option value="gemini-2.5-pro">gemini-2.5-pro</option>
                <option value="claude-3-7-sonnet">claude-3-7-sonnet</option>
                <option value="gpt-4o">gpt-4o</option>
              </>
            )}
          </select>
          <span style={{ fontSize: 12, color: "var(--muted)" }}>
            {t("Routes to")}: <code style={{ color: "var(--brand)" }}>{liveModels.some((model) => model.id === testModel) ? testModel : testModel.includes("claude") ? "claude-sonnet-5-5-medium" : testModel.includes("gpt") ? "gpt-oss-120b-medium" : testModel.includes("pro") ? "gemini-pro-agent" : "gemini-3.6-flash-high"}</code>
          </span>
        </div>

        <div style={{ display: "flex", gap: 10 }}>
          <input
            type="text"
            className="search-input"
            value={testPrompt}
            onChange={(e) => setTestPrompt(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void handleSendTest();
            }}
            placeholder={t("Type a test prompt...")}
            style={{ flex: 1 }}
          />
          <button
            type="button"
            className="primary-button"
            onClick={() => void handleSendTest()}
            disabled={testLoading}
          >
            {testLoading ? <RefreshCw size={15} className="spin" /> : <Send size={15} />}
            {t("Send test request")}
          </button>
        </div>

        {testError && (
          <div className="error-banner" style={{ marginTop: 14 }}>
            <ShieldAlert size={14} />
            <span>{testError}</span>
          </div>
        )}

        {testResult && (
          <div
            style={{
              marginTop: 16,
              padding: 14,
              borderRadius: 8,
              background: "rgba(0,0,0,0.3)",
              border: "1px solid rgba(255,255,255,0.08)",
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 8, fontSize: 12 }}>
              <span style={{ color: "#dba8c2", display: "flex", alignItems: "center", gap: 4 }}>
                <CheckCircle2 size={13} /> 200 OK
              </span>
              <span style={{ color: "var(--muted)" }}>
                <Clock3 size={12} style={{ display: "inline", verticalAlign: "middle", marginRight: 3 }} />
                {testResult.latencyMs} ms
              </span>
            </div>
            <p style={{ margin: 0, fontSize: 13, lineHeight: 1.5, color: "#f8fafc", whiteSpace: "pre-wrap" }}>
              {testResult.text}
            </p>
          </div>
        )}
      </div>

      {/* LIVE TRAFFIC INSPECTOR */}
      <div className="section-heading" style={{ marginTop: 24 }}>
        <div>
          <span className="eyebrow">{t("Monitoring")}</span>
          <h2>{t("Live Gateway Traffic")}</h2>
        </div>
        {status?.recentLogs && status.recentLogs.length > 0 && (
          <button
            type="button"
            className="secondary-button"
            style={{ fontSize: 12, padding: "4px 10px" }}
            onClick={async () => {
              await api.clearProxyLogs();
              await fetchStatus();
            }}
          >
            <Trash2 size={13} />
            {t("Clear traffic logs")}
          </button>
        )}
      </div>

      <div className="glass-panel" style={{ padding: 0, overflow: "hidden", marginBottom: 24 }}>
        {status?.recentLogs && status.recentLogs.length > 0 ? (
          <div style={{ overflowX: "auto" }}>
            <table className="accounts-table" style={{ margin: 0 }}>
              <thead>
                <tr>
                  <th style={{ width: 85 }}>{t("Time")}</th>
                  <th style={{ width: 100 }}>{t("Protocol")}</th>
                  <th>{t("Model Routing")}</th>
                  <th>{t("Account")}</th>
                  <th style={{ width: 90 }}>{t("Status")}</th>
                  <th style={{ width: 85, textAlign: "right" }}>{t("Latency")}</th>
                </tr>
              </thead>
              <tbody>
                {status.recentLogs.map((log) => {
                  const isDowngraded = log.requestedModel.includes("haiku") && log.upstreamModel.includes("flash");
                  const dateStr = new Date(log.timestamp * 1000).toLocaleTimeString();
                  return (
                    <tr key={log.id}>
                      <td style={{ fontSize: 12, color: "var(--muted)", fontFamily: "monospace" }}>
                        {dateStr}
                      </td>
                      <td>
                        <span
                          className="status-chip"
                          style={{
                            fontSize: 11,
                            textTransform: "uppercase",
                            letterSpacing: "0.5px",
                            padding: "2px 8px",
                            background:
                              log.clientProtocol === "anthropic"
                                ? "rgba(217, 119, 6, 0.15)"
                                : log.clientProtocol === "openai"
                                ? "rgba(219, 168, 194, 0.15)"
                                : "rgba(59, 130, 246, 0.15)",
                            color:
                              log.clientProtocol === "anthropic"
                                ? "#f59e0b"
                                : log.clientProtocol === "openai"
                                ? "#dba8c2"
                                : "#60a5fa",
                            border: "none",
                          }}
                        >
                          {log.clientProtocol}
                          {log.isStream && <span style={{ marginLeft: 4, opacity: 0.7 }}>SSE</span>}
                        </span>
                      </td>
                      <td>
                        <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12 }}>
                          <span style={{ fontFamily: "monospace", color: "#f8fafc" }}>{log.requestedModel}</span>
                          <ArrowRight size={12} color="var(--muted)" />
                          <span style={{ fontFamily: "monospace", color: "var(--accent)" }}>{log.upstreamModel}</span>
                          {isDowngraded && (
                            <span
                              className="status-chip"
                              style={{
                                fontSize: 10,
                                background: "rgba(219, 168, 194, 0.2)",
                                color: "#dba8c2",
                                border: "none",
                                padding: "1px 6px",
                              }}
                            >
                              Quota Saver
                            </span>
                          )}
                        </div>
                      </td>
                      <td style={{ fontSize: 12 }}>
                        {log.accountEmail && log.accountEmail !== "-" ? (
                          <span style={{ color: "var(--muted)" }}>{log.accountEmail}</span>
                        ) : (
                          <span style={{ color: "var(--muted)", opacity: 0.5 }}>—</span>
                        )}
                      </td>
                      <td>
                        <span
                          className={`status-chip ${
                            log.statusCode === 200
                              ? "status-chip--connected"
                              : log.statusCode === 429
                              ? "status-chip--warning"
                              : "status-chip--error"
                          }`}
                          style={{ fontSize: 11, padding: "2px 8px" }}
                        >
                          {log.statusCode === 200 ? "200 OK" : log.statusCode}
                        </span>
                      </td>
                      <td style={{ fontSize: 12, textAlign: "right", fontFamily: "monospace", color: "var(--muted)" }}>
                        {log.latencyMs} ms
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        ) : (
          <div style={{ padding: "32px 20px", textAlign: "center", color: "var(--muted)" }}>
            <Activity size={28} style={{ opacity: 0.3, marginBottom: 8, display: "inline-block" }} />
            <p style={{ margin: "0 0 4px", fontSize: 13, fontWeight: 500, color: "#f8fafc" }}>
              {t("No gateway traffic recorded yet")}
            </p>
            <p style={{ margin: 0, fontSize: 12 }}>
              {t("Requests sent through the proxy from Claude Code, OpenAI clients, or the test playground will appear here live.")}
            </p>
          </div>
        )}
      </div>

      {/* MATRIX DISPATCH FEATURES (lbjlaq & Draculabo inspired) */}
      <div className="section-heading">
        <div>
          <span className="eyebrow">{t("Architecture")}</span>
          <h2>{t("Proxy Matrix Features")}</h2>
        </div>
      </div>

      <div className="integration-grid">
        <div className="glass-panel" style={{ padding: 16 }}>
          <Zap size={20} color="#f59e0b" style={{ marginBottom: 8 }} />
          <h3 style={{ margin: "0 0 6px", fontSize: 14 }}>{t("Smart Self-Healing (429 Failover)")}</h3>
          <p style={{ margin: 0, fontSize: 12, color: "var(--muted)", lineHeight: 1.5 }}>
            {t("When an account encounters a 429 rate-limit, the proxy silently rotates to the next available account in milliseconds.")}
          </p>
        </div>

        <div className="glass-panel" style={{ padding: 16 }}>
          <Flame size={20} color="#3b82f6" style={{ marginBottom: 8 }} />
          <h3 style={{ margin: "0 0 6px", fontSize: 14 }}>{t("Silent Background Downgrading")}</h3>
          <p style={{ margin: 0, fontSize: 12, color: "var(--muted)", lineHeight: 1.5 }}>
            {t("Background tasks (like Claude CLI title generation) are redirected to Flash models to preserve premium Claude Sonnet quota.")}
          </p>
        </div>

        <div className="glass-panel" style={{ padding: 16 }}>
          <ShieldAlert size={20} color="#ef4444" style={{ marginBottom: 8 }} />
          <h3 style={{ margin: "0 0 6px", fontSize: 14 }}>{t("Smart 403 Forbidden Detection")}</h3>
          <p style={{ margin: 0, fontSize: 12, color: "var(--muted)", lineHeight: 1.5 }}>
            {t("Accounts with permission anomalies or project bans are automatically isolated from the proxy rotation pool.")}
          </p>
        </div>
      </div>
    </section>
  );
}
