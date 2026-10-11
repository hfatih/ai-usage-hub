import {
  AlertTriangle,
  ArrowRightLeft,
  ArrowUpDown,
  Check,
  Clock3,
  Download,
  FileJson,
  LayoutGrid,
  List,
  LogIn,
  Plus,
  RefreshCw,
  Search,
  ShieldCheck,
  Star,
  Trash2,
  Upload,
  UserRoundCheck,
  X,
  Zap,
} from "lucide-react";
import { useState } from "react";
import { formatUsageValue, relativeTime, resetTime, shortResetTime, sourceLabel, usageProgress, usageTiming } from "../lib/format";
import { localizeValue, useTranslation } from "../i18n";
import { useHub } from "../stores/hub";

export function AccountsPage() {
  const { language, t } = useTranslation();
  const {
    accounts,
    antigravityAccounts,
    settings,
    deleteAccount,
    refresh,
    refreshing,
    captureAntigravityAccount,
    addAntigravityAccount,
    deleteAntigravityAccount,
    switchAntigravityAccount,
    toggleAntigravityAutoSwitch,
    startAntigravityOAuthLogin,
    completeAntigravityOAuthCode,
    exportAntigravityAccounts,
    importAntigravityAccounts,
    refreshAllAntigravityAccounts,
    scanLocalAntigravityAccounts,
    liveSession,
    syncAntigravitySession,
  } = useHub();

  const [viewMode, setViewMode] = useState<"grid" | "list">("grid");
  const [switchingId, setSwitchingId] = useState<string | null>(null);
  const [capturing, setCapturing] = useState(false);
  const [oauthLoading, setOauthLoading] = useState(false);
  const [oauthBanner, setOauthBanner] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [sortBy, setSortBy] = useState<"quota" | "recent" | "email" | "status">("quota");
  const [refreshingAll, setRefreshingAll] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [syncingSession, setSyncingSession] = useState(false);
  const [batchBanner, setBatchBanner] = useState<string | null>(null);

  // Add Account Modal (Manual Refresh Token or Code)
  const [showAddModal, setShowAddModal] = useState(false);
  const [addEmail, setAddEmail] = useState("");
  const [addRefreshToken, setAddRefreshToken] = useState("");
  const [manualCode, setManualCode] = useState("");
  const [addLoading, setAddLoading] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);

  // Import Modal
  const [showImportModal, setShowImportModal] = useState(false);
  const [importJson, setImportJson] = useState("");
  const [importLoading, setImportLoading] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);

  // Best account recommendation (Highest healthy remaining quota)
  const bestAccountId = antigravityAccounts
    .filter((a) => a.status === "healthy" && (a.quotaRemainingPercent ?? 0) > 0)
    .sort((a, b) => (b.quotaRemainingPercent ?? 0) - (a.quotaRemainingPercent ?? 0))[0]?.id;

  // Filtered and sorted accounts list
  const displayedAccounts = antigravityAccounts
    .filter((a) => {
      if (!searchQuery.trim()) return true;
      const q = searchQuery.toLowerCase();
      return a.email.toLowerCase().includes(q) || (a.displayName?.toLowerCase().includes(q) ?? false);
    })
    .sort((a, b) => {
      if (sortBy === "quota") {
        return (b.quotaRemainingPercent ?? 0) - (a.quotaRemainingPercent ?? 0);
      }
      if (sortBy === "recent") {
        return b.lastUsed - a.lastUsed;
      }
      if (sortBy === "email") {
        return a.email.localeCompare(b.email);
      }
      if (sortBy === "status") {
        return a.status.localeCompare(b.status);
      }
      return 0;
    });

  async function handleRefreshAll() {
    try {
      setRefreshingAll(true);
      await refreshAllAntigravityAccounts();
      setBatchBanner(t("Quotas refreshed"));
      setTimeout(() => setBatchBanner(null), 3000);
    } catch (err) {
      alert(String(err));
    } finally {
      setRefreshingAll(false);
    }
  }

  async function handleScanLocal() {
    try {
      setScanning(true);
      await scanLocalAntigravityAccounts();
      setBatchBanner(t("Local account scanned"));
      setTimeout(() => setBatchBanner(null), 3000);
    } catch (err) {
      alert(String(err));
    } finally {
      setScanning(false);
    }
  }

  async function handleSyncSession() {
    try {
      setSyncingSession(true);
      const session = await syncAntigravitySession();
      setBatchBanner(
        t("Antigravity session verified and synced: {email}", {
          email: session.email || t("No active session found"),
        })
      );
      setTimeout(() => setBatchBanner(null), 3500);
    } catch (err) {
      alert(String(err));
    } finally {
      setSyncingSession(false);
    }
  }

  async function removeGeneric(id: string, email: string) {
    if (!window.confirm(t("Delete {email} and its local usage history from AI Usage Hub? The provider account itself will not be deleted.", { email }))) return;
    await deleteAccount(id);
  }

  async function removeAntigravity(id: string, email: string) {
    if (!window.confirm(t("Delete {email} from Antigravity account pool?", { email }))) return;
    await deleteAntigravityAccount(id);
  }

  async function handleSwitch(id: string) {
    try {
      setSwitchingId(id);
      await switchAntigravityAccount(id);
    } catch (err) {
      alert(String(err));
    } finally {
      setSwitchingId(null);
    }
  }

  async function handleCapture() {
    try {
      setCapturing(true);
      await captureAntigravityAccount();
    } catch (err) {
      alert(String(err));
    } finally {
      setCapturing(false);
    }
  }

  async function handleOAuthLogin() {
    try {
      setOauthLoading(true);
      setOauthBanner(t("Waiting for browser login... (Port 8045)"));
      await startAntigravityOAuthLogin();
      setOauthBanner(null);
    } catch (err) {
      alert(String(err));
    } finally {
      setOauthLoading(false);
      setOauthBanner(null);
    }
  }

  async function handleExport() {
    try {
      const json = await exportAntigravityAccounts();
      if (navigator.clipboard) {
        await navigator.clipboard.writeText(json);
      }
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `antigravity-accounts-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
      URL.revokeObjectURL(url);
      alert(t("Accounts exported successfully"));
    } catch (err) {
      alert(String(err));
    }
  }

  async function handleImportSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!importJson.trim()) return;
    try {
      setImportLoading(true);
      setImportError(null);
      const count = await importAntigravityAccounts(importJson.trim());
      alert(t("Import completed: {count} accounts", { count }));
      setImportJson("");
      setShowImportModal(false);
    } catch (err) {
      setImportError(String(err));
    } finally {
      setImportLoading(false);
    }
  }

  async function handleImportFile(file: File) {
    try {
      const text = await file.text();
      setImportJson(text);
    } catch (err) {
      setImportError(String(err));
    }
  }

  async function handleAddSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (manualCode.trim()) {
      try {
        setAddLoading(true);
        setAddError(null);
        await completeAntigravityOAuthCode(manualCode.trim());
        setManualCode("");
        setShowAddModal(false);
      } catch (err) {
        setAddError(String(err));
      } finally {
        setAddLoading(false);
      }
      return;
    }

    if (!addEmail.trim() || !addRefreshToken.trim()) return;
    try {
      setAddLoading(true);
      setAddError(null);
      await addAntigravityAccount(addEmail.trim(), addRefreshToken.trim());
      setAddEmail("");
      setAddRefreshToken("");
      setShowAddModal(false);
    } catch (err) {
      setAddError(String(err));
    } finally {
      setAddLoading(false);
    }
  }

  return (
    <section>
      {/* ANTIGRAVITY POOL & AUTO-SWITCHER SECTION */}
      <div className="page-heading">
        <div>
          <span className="eyebrow">{t("Auto-Switcher")}</span>
          <h1>{t("Antigravity Account Pool")}</h1>
          <p>{t("Auto-switch accounts when quota runs low or rate limits occur.")}</p>
        </div>
        <div className="hero-actions">
          {/* View Mode Toggle */}
          <div className="view-mode-group">
            <button
              type="button"
              className={`view-mode-btn ${viewMode === "grid" ? "is-active" : ""}`}
              onClick={() => setViewMode("grid")}
              title={t("Grid view")}
            >
              <LayoutGrid size={14} />
            </button>
            <button
              type="button"
              className={`view-mode-btn ${viewMode === "list" ? "is-active" : ""}`}
              onClick={() => setViewMode("list")}
              title={t("List view")}
            >
              <List size={14} />
            </button>
          </div>

          <button
            type="button"
            className={`auto-switch-toggle ${settings?.antigravityAutoSwitch ? "is-active" : ""}`}
            onClick={() => void toggleAntigravityAutoSwitch(!settings?.antigravityAutoSwitch)}
            title={settings?.antigravityAutoSwitch ? t("Auto-switch enabled") : t("Auto-switch disabled")}
          >
            <Zap size={14} />
            <span>{settings?.antigravityAutoSwitch ? t("Auto-switch enabled") : t("Auto-switch disabled")}</span>
          </button>

          <button
            type="button"
            className="secondary-button"
            onClick={() => void handleCapture()}
            disabled={capturing}
            title={t("Capture active account credentials from Antigravity IDE")}
          >
            <Download className={capturing ? "spin" : ""} size={14} />
            {t("Capture from IDE")}
          </button>

          <button
            type="button"
            className="secondary-button"
            onClick={() => void refresh()}
            disabled={refreshing}
            title={t("Refresh pool quotas")}
          >
            <RefreshCw className={refreshing ? "spin" : ""} size={14} />
            {t("Refresh")}
          </button>

          <button
            type="button"
            className="secondary-button"
            onClick={() => void handleOAuthLogin()}
            disabled={oauthLoading}
            title={t("Sign in via official Google OAuth")}
          >
            <LogIn className={oauthLoading ? "spin" : ""} size={14} />
            {oauthLoading ? t("Waiting for browser login...") : t("Sign in with Google")}
          </button>

          <button
            type="button"
            className="secondary-button"
            onClick={() => void handleExport()}
            title={t("Export Accounts")}
          >
            <Upload size={14} />
            {t("Export")}
          </button>

          <button
            type="button"
            className="secondary-button"
            onClick={() => setShowImportModal(true)}
            title={t("Import Accounts")}
          >
            <FileJson size={14} />
            {t("Import")}
          </button>

          <button
            type="button"
            className="primary-button"
            onClick={() => setShowAddModal(true)}
          >
            <Plus size={14} />
            {t("Add Account")}
          </button>
        </div>
      </div>

      {oauthBanner && (
        <div className="glass-panel" style={{ padding: "12px 16px", marginBottom: 16, display: "flex", alignItems: "center", gap: 10, background: "rgba(56, 189, 248, 0.1)", border: "1px solid rgba(56, 189, 248, 0.3)" }}>
          <RefreshCw className="spin" size={16} style={{ color: "#38bdf8" }} />
          <span style={{ fontSize: 13, color: "#e0f2fe" }}>{oauthBanner}</span>
        </div>
      )}

      {batchBanner && (
        <div className="glass-panel" style={{ padding: "12px 16px", marginBottom: 16, display: "flex", alignItems: "center", gap: 10, background: "rgba(16, 185, 129, 0.15)", border: "1px solid rgba(16, 185, 129, 0.3)" }}>
          <Check size={16} style={{ color: "#10b981" }} />
          <span style={{ fontSize: 13, color: "#d1fae5" }}>{batchBanner}</span>
        </div>
      )}

      {/* LIVE ANTIGRAVITY SESSION STATUS CARD */}
      <div className="live-session-card">
        <div className="live-session-header">
          <div className="live-session-badge">
            <span className={`status-indicator ${liveSession?.isRunning ? "status-indicator--live" : "status-indicator--idle"}`} />
            <strong>
              {liveSession?.isRunning
                ? t("Antigravity IDE Connected")
                : t("Antigravity IDE Closed (System Vault)")}
            </strong>
          </div>
          <button
            type="button"
            className="sync-session-btn"
            onClick={() => void handleSyncSession()}
            disabled={syncingSession}
            title={t("Sync & Verify Live Session")}
          >
            <RefreshCw className={syncingSession ? "spin" : ""} size={13} />
            <span>{syncingSession ? t("Syncing session...") : t("Sync & Verify Live Session")}</span>
          </button>
        </div>

        <div className="live-session-content">
          <div className="live-session-field">
            <small>{t("Verified Active Account")}</small>
            <span className="live-email">{liveSession?.email || t("No active session found")}</span>
          </div>
          <div className="live-session-field">
            <small>{t("Subscription / Tier")}</small>
            <span>{liveSession?.plan || "Google AI"}</span>
          </div>
          <div className="live-session-field">
            <small>{t("Connection Source")}</small>
            <span className="live-source-pill">
              {liveSession?.source || t("Not detected")}
            </span>
          </div>
          {liveSession?.quotaRemainingPercent != null && (
            <div className="live-session-field">
              <small>{t("Instant Usable Quota")}</small>
              <span className="live-quota-val">
                %{Math.round(liveSession.quotaRemainingPercent)}
              </span>
            </div>
          )}
        </div>
      </div>

      {/* FILTER & BATCH OPERATIONS TOOLBAR */}
      {antigravityAccounts.length > 0 && (
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 12, marginBottom: 18, flexWrap: "wrap" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, flex: 1, minWidth: 220 }}>
            <div style={{ position: "relative", width: "100%", maxWidth: 300 }}>
              <Search size={14} style={{ position: "absolute", left: 10, top: "50%", transform: "translateY(-50%)", color: "var(--muted)" }} />
              <input
                type="text"
                className="search-input"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t("Search accounts...")}
                style={{ paddingLeft: 32, width: "100%", height: 32, fontSize: 12 }}
              />
            </div>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
            <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, color: "var(--muted)" }}>
              <ArrowUpDown size={12} />
              <span>{t("Sort by")}:</span>
              <select
                value={sortBy}
                onChange={(e) => setSortBy(e.target.value as any)}
                style={{ width: "auto", height: 32, fontSize: 12, padding: "0 8px" }}
              >
                <option value="quota">{t("Highest quota")}</option>
                <option value="recent">{t("Recently used")}</option>
                <option value="email">{t("Email (A-Z)")}</option>
                <option value="status">{t("Status")}</option>
              </select>
            </div>

            <button
              type="button"
              className="secondary-button"
              style={{ height: 32, fontSize: 12, padding: "4px 10px" }}
              onClick={() => void handleRefreshAll()}
              disabled={refreshingAll}
              title={t("Refresh all quotas")}
            >
              <RefreshCw className={refreshingAll ? "spin" : ""} size={13} />
              {refreshingAll ? t("Refreshing quotas...") : t("Refresh all quotas")}
            </button>

            <button
              type="button"
              className="secondary-button"
              style={{ height: 32, fontSize: 12, padding: "4px 10px" }}
              onClick={() => void handleScanLocal()}
              disabled={scanning}
              title={t("Scan system account")}
            >
              <Download className={scanning ? "spin" : ""} size={13} />
              {scanning ? t("Scanning...") : t("Scan system account")}
            </button>
          </div>
        </div>
      )}

      {displayedAccounts.length === 0 ? (
        <div className="glass-panel empty-state">
          <UserRoundCheck size={28} />
          <strong>{antigravityAccounts.length === 0 ? t("No accounts in pool") : t("No accounts matched your search")}</strong>
          <span>{antigravityAccounts.length === 0 ? t("Capture the active account from your Antigravity IDE or add one with a refresh token.") : t("Try another search query.")}</span>
        </div>
      ) : viewMode === "grid" ? (
        <div className="account-grid pool-grid">
          {displayedAccounts.map((account) => {
            const isBest = account.id === bestAccountId && !account.isCurrent;
            return (
              <article className={`glass-panel account-card ${account.isCurrent ? "is-current-pool-card" : ""}`} key={account.id}>
                <header>
                  <span className="provider-logo provider-logo--google">G</span>
                  <div>
                    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                      <small>Google / Antigravity</small>
                      {isBest && (
                        <span className="status-badge" style={{ background: "rgba(245, 158, 11, 0.15)", color: "#fbbf24", border: "1px solid rgba(245, 158, 11, 0.3)" }}>
                          <Star size={10} /> {t("Best Account")}
                        </span>
                      )}
                    </div>
                    <strong>{account.email}</strong>
                    <span>{account.displayName || t("Google account")}</span>
                  </div>
                  {account.isCurrent ? (
                    <span className="connection-pill connection-pill--connected" style={{ background: "rgba(16, 185, 129, 0.15)", border: "1px solid rgba(16, 185, 129, 0.4)", color: "#34d399", fontWeight: 700 }}>
                      <Check size={12} /> {t("Verified IDE Session")}
                    </span>
                  ) : (
                    <button
                      type="button"
                      className="secondary-button switch-btn"
                      onClick={() => void handleSwitch(account.id)}
                      disabled={switchingId === account.id}
                    >
                      <ArrowRightLeft size={13} className={switchingId === account.id ? "spin" : ""} />
                      {switchingId === account.id ? t("Switching...") : t("Switch")}
                    </button>
                  )}
                </header>

                <div className="account-usage-list">
                  <div className="account-usage">
                    <div>
                      <span>{t("Quota remaining")}</span>
                      <strong>
                        {account.quotaRemainingPercent != null
                          ? account.quotaRemainingPercent > 0 && account.quotaRemainingPercent < 1
                            ? "<1%"
                            : `${Math.round(account.quotaRemainingPercent)}%`
                          : t("Calculated")}
                      </strong>
                    </div>
                    {account.quotaRemainingPercent != null && (
                      <div className="usage-track">
                        <span style={{ width: `${Math.max(0, Math.min(100, account.quotaRemainingPercent))}%` }} />
                      </div>
                    )}
                    <small>
                      {account.status === "forbidden" ? (
                        <span style={{ color: "#ef4444", display: "inline-flex", alignItems: "center", gap: 3 }}>
                          <AlertTriangle size={11} /> {t("Forbidden (403)")}
                        </span>
                      ) : account.status === "rate_limited" ? (
                        <>
                          <ShieldCheck size={11} /> {t("Rate limited")}
                        </>
                      ) : (
                        <>
                          <ShieldCheck size={11} /> {t("Healthy")}
                        </>
                      )}
                    </small>

                    {account.quotaDetails && (
                      <div className="account-model-breakdown">
                        <div
                          className="model-quota-chip"
                          title={account.quotaDetails.geminiResetAt ? resetTime(account.quotaDetails.geminiResetAt) : undefined}
                        >
                          <div className="model-quota-chip-header">
                            <span className="model-quota-label">Gemini 5H</span>
                            <strong className="model-quota-val">
                              {account.quotaDetails.gemini5h != null ? `${Math.round(account.quotaDetails.gemini5h)}%` : "—"}
                            </strong>
                          </div>
                          {account.quotaDetails.geminiResetAt && account.quotaDetails.gemini5h != null && account.quotaDetails.gemini5h < 100 && (
                            <span className="model-quota-reset">
                              🕒 {shortResetTime(account.quotaDetails.geminiResetAt)}
                            </span>
                          )}
                        </div>
                        <div className="model-quota-chip">
                          <div className="model-quota-chip-header">
                            <span className="model-quota-label">Gemini W</span>
                            <strong className="model-quota-val">
                              {account.quotaDetails.geminiWeekly != null ? `${Math.round(account.quotaDetails.geminiWeekly)}%` : "—"}
                            </strong>
                          </div>
                        </div>
                        <div
                          className="model-quota-chip"
                          title={account.quotaDetails.claudeResetAt ? resetTime(account.quotaDetails.claudeResetAt) : undefined}
                        >
                          <div className="model-quota-chip-header">
                            <span className="model-quota-label">Claude 5H</span>
                            <strong className="model-quota-val">
                              {account.quotaDetails.claude5h != null ? `${Math.round(account.quotaDetails.claude5h)}%` : "—"}
                            </strong>
                          </div>
                          {account.quotaDetails.claudeResetAt && account.quotaDetails.claude5h != null && account.quotaDetails.claude5h < 100 && (
                            <span className="model-quota-reset">
                              🕒 {shortResetTime(account.quotaDetails.claudeResetAt)}
                            </span>
                          )}
                        </div>
                        <div className="model-quota-chip">
                          <div className="model-quota-chip-header">
                            <span className="model-quota-label">Claude W</span>
                            <strong className="model-quota-val">
                              {account.quotaDetails.claudeWeekly != null
                                ? account.quotaDetails.claudeWeekly > 0 && account.quotaDetails.claudeWeekly < 1
                                  ? "<1%"
                                  : `${Math.round(account.quotaDetails.claudeWeekly)}%`
                                : "—"}
                            </strong>
                          </div>
                        </div>
                      </div>
                    )}
                  </div>
                </div>

                <footer>
                  <span>
                    <Clock3 size={12} />
                    {account.isCurrent ? t("Active now") : `${t("Last known snapshot")} · ${relativeTime(account.lastUsed)}`}
                  </span>
                  <button
                    type="button"
                    className="danger-button"
                    onClick={() => void removeAntigravity(account.id, account.email)}
                    title={t("Delete")}
                  >
                    <Trash2 size={14} />
                    {t("Delete")}
                  </button>
                </footer>
              </article>
            );
          })}
        </div>
      ) : (
        /* LIST / TABLE VIEW */
        <div className="glass-panel account-table-container">
          <table className="account-table">
            <thead>
              <tr>
                <th>{t("Account")}</th>
                <th>{t("Status")}</th>
                <th>{t("Quota remaining")}</th>
                <th>{t("Last known snapshot")}</th>
                <th style={{ textAlign: "right" }}>{t("Actions")}</th>
              </tr>
            </thead>
            <tbody>
              {displayedAccounts.map((account) => {
                const isBest = account.id === bestAccountId && !account.isCurrent;
                return (
                  <tr key={account.id} className={account.isCurrent ? "is-current-row" : ""}>
                    <td>
                      <div className="table-account-cell">
                        <span className="provider-logo provider-logo--google">G</span>
                        <div>
                          <strong>{account.email}</strong>
                          <small>{account.displayName || "Google"}</small>
                        </div>
                        {isBest && (
                          <span className="status-badge" style={{ background: "rgba(245, 158, 11, 0.15)", color: "#fbbf24", border: "1px solid rgba(245, 158, 11, 0.3)" }}>
                            <Star size={10} /> {t("Best Account")}
                          </span>
                        )}
                      </div>
                    </td>
                    <td>
                      {account.isCurrent ? (
                        <span className="connection-pill connection-pill--connected" style={{ background: "rgba(16, 185, 129, 0.15)", border: "1px solid rgba(16, 185, 129, 0.4)", color: "#34d399", fontWeight: 700 }}>
                          <Check size={11} /> {t("Verified IDE Session")}
                        </span>
                      ) : account.status === "forbidden" ? (
                        <span className="status-badge" style={{ background: "rgba(239, 68, 68, 0.15)", color: "#f87171", border: "1px solid rgba(239, 68, 68, 0.3)" }}>
                          <AlertTriangle size={10} /> {t("Forbidden (403)")}
                        </span>
                      ) : account.status === "rate_limited" ? (
                        <span className="status-badge status-badge--limited">
                          {t("Rate limited")}
                        </span>
                      ) : (
                        <span className="status-badge status-badge--healthy">
                          {t("Healthy")}
                        </span>
                      )}
                    </td>
                    <td>
                      <div className="table-quota-cell">
                        <span>
                          {account.quotaRemainingPercent != null
                            ? account.quotaRemainingPercent > 0 && account.quotaRemainingPercent < 1
                              ? "<1%"
                              : `${Math.round(account.quotaRemainingPercent)}%`
                            : t("Calculated")}
                        </span>
                        {account.quotaRemainingPercent != null && (
                          <div className="usage-track-sm">
                            <span style={{ width: `${Math.max(0, Math.min(100, account.quotaRemainingPercent))}%` }} />
                          </div>
                        )}
                        {account.quotaDetails && (
                          <div className="table-model-chips">
                            <span title={account.quotaDetails.geminiResetAt ? resetTime(account.quotaDetails.geminiResetAt) : undefined}>
                              G 5H: {account.quotaDetails.gemini5h != null ? `${Math.round(account.quotaDetails.gemini5h)}%` : "—"}
                              {account.quotaDetails.geminiResetAt && account.quotaDetails.gemini5h != null && account.quotaDetails.gemini5h < 100 && (
                                ` (${shortResetTime(account.quotaDetails.geminiResetAt)})`
                              )}
                            </span>
                            <span title={account.quotaDetails.claudeResetAt ? resetTime(account.quotaDetails.claudeResetAt) : undefined}>
                              C 5H: {account.quotaDetails.claude5h != null ? `${Math.round(account.quotaDetails.claude5h)}%` : "—"}
                              {account.quotaDetails.claudeResetAt && account.quotaDetails.claude5h != null && account.quotaDetails.claude5h < 100 && (
                                ` (${shortResetTime(account.quotaDetails.claudeResetAt)})`
                              )}
                            </span>
                          </div>
                        )}
                      </div>
                    </td>
                    <td>
                      <span style={{ fontSize: 12, color: "var(--muted)" }}>
                        {account.isCurrent ? t("Active now") : relativeTime(account.lastUsed)}
                      </span>
                    </td>
                    <td>
                      <div className="table-actions">
                        {!account.isCurrent && (
                          <button
                            type="button"
                            className="secondary-button-sm"
                            onClick={() => void handleSwitch(account.id)}
                            disabled={switchingId === account.id}
                          >
                            <ArrowRightLeft size={11} className={switchingId === account.id ? "spin" : ""} />
                            {switchingId === account.id ? t("Switching...") : t("Switch")}
                          </button>
                        )}
                        <button
                          type="button"
                          className="danger-button-sm"
                          onClick={() => void removeAntigravity(account.id, account.email)}
                          title={t("Delete")}
                        >
                          <Trash2 size={12} />
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      <p className="page-note">
        {t("Hot-Switching restarts the language server in seconds without closing open IDE editor tabs or projects.")}
      </p>

      {/* GENERAL SAVED ACCOUNTS SECTION */}
      <div className="section-divider" />
      <div className="page-heading">
        <div>
          <span className="eyebrow">{t("Saved accounts")}</span>
          <h2>{t("Accounts")}</h2>
          <p>{t("Active and previously seen accounts retain their full email and latest verified usage values.")}</p>
        </div>
        <button className="primary-button" onClick={() => void refresh()} disabled={refreshing}>
          <RefreshCw className={refreshing ? "spin" : ""} size={16} />
          {t("Refresh active accounts")}
        </button>
      </div>

      {accounts.length === 0 ? (
        <div className="glass-panel empty-state">
          <UserRoundCheck size={28} />
          <strong>{t("No saved accounts yet")}</strong>
          <span>{t("Sign in to Codex, Claude Code or Antigravity, then select Refresh all.")}</span>
        </div>
      ) : (
        <div className="account-grid">
          {accounts.map((account) => (
            <article className="glass-panel account-card" key={account.id}>
              <header>
                <span className={`provider-logo provider-logo--${account.providerId}`}>
                  {account.providerName.slice(0, 1)}
                </span>
                <div>
                  <small>{account.providerName}</small>
                  <strong>{account.email}</strong>
                  <span>{account.plan ? localizeValue(account.plan, language) : t("Plan not reported")}</span>
                </div>
                <span className={`connection-pill connection-pill--${account.connectionStatus}`}>
                  {t(account.connectionStatus === "connected" ? "Active" : "Saved")}
                </span>
              </header>
              <div className="account-usage-list">
                {account.usage.length ? (
                  account.usage.map((usage) => {
                    const percent = usageProgress(usage);
                    return (
                      <div className="account-usage" key={usage.metric}>
                        <div>
                          <span>{localizeValue(usage.label, language)}</span>
                          <strong>{formatUsageValue(usage)}</strong>
                        </div>
                        {percent != null && (
                          <div className="usage-track">
                            <span style={{ width: `${percent}%` }} />
                          </div>
                        )}
                        <small>
                          <ShieldCheck size={11} />
                          {sourceLabel(usage.source)} · {usageTiming(usage)}
                        </small>
                      </div>
                    );
                  })
                ) : (
                  <div className="account-empty-usage">
                    {t("No programmatic quota source available for this account.")}
                  </div>
                )}
              </div>
              <footer>
                <span>
                  <Clock3 size={12} />
                  {t(account.connectionStatus === "connected" ? "Active now" : "Last known snapshot")} ·{" "}
                  {relativeTime(account.lastRefresh)}
                </span>
                <button
                  type="button"
                  className="danger-button"
                  onClick={() => void removeGeneric(account.id, account.email)}
                >
                  <Trash2 size={14} />
                  {t("Delete")}
                </button>
              </footer>
            </article>
          ))}
        </div>
      )}
      <p className="page-note">
        {t("Signed-out accounts cannot be refreshed live without retaining their credentials. Their last verified values and timestamps remain visible.")}
      </p>

      {/* ADD ACCOUNT MODAL */}
      {showAddModal && (
        <div className="modal-backdrop" onClick={() => setShowAddModal(false)}>
          <div className="glass-panel modal-card" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h3>{t("Add Account")}</h3>
              <button type="button" className="icon-button" onClick={() => setShowAddModal(false)}>
                <X size={16} />
              </button>
            </div>
            <form onSubmit={handleAddSubmit}>
              {addError && <div className="error-banner">{addError}</div>}
              <div className="modal-body">
                <label>
                  <span>{t("Enter your Google email")}</span>
                  <input
                    type="email"
                    placeholder="account@gmail.com"
                    value={addEmail}
                    onChange={(e) => setAddEmail(e.target.value)}
                  />
                </label>
                <label>
                  <span>{t("Google Refresh Token")}</span>
                  <textarea
                    rows={3}
                    placeholder="1//..."
                    value={addRefreshToken}
                    onChange={(e) => setAddRefreshToken(e.target.value)}
                  />
                </label>
                <div style={{ textAlign: "center", margin: "8px 0", color: "var(--muted)", fontSize: 12 }}>
                  — {t("Or enter authorization code manually")} —
                </div>
                <label>
                  <span>{t("Authorization code")}</span>
                  <input
                    type="text"
                    placeholder="4/0A..."
                    value={manualCode}
                    onChange={(e) => setManualCode(e.target.value)}
                  />
                </label>
              </div>
              <div className="modal-actions">
                <button
                  type="button"
                  className="secondary-button"
                  onClick={() => setShowAddModal(false)}
                >
                  {t("Cancel")}
                </button>
                <button
                  type="submit"
                  className="primary-button"
                  disabled={addLoading}
                >
                  {addLoading ? t("Switching...") : t("Save Account")}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* IMPORT JSON MODAL */}
      {showImportModal && (
        <div className="modal-backdrop" onClick={() => setShowImportModal(false)}>
          <div className="glass-panel modal-card" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h3>{t("Import Accounts")}</h3>
              <button type="button" className="icon-button" onClick={() => setShowImportModal(false)}>
                <X size={16} />
              </button>
            </div>
            <form onSubmit={handleImportSubmit}>
              {importError && <div className="error-banner">{importError}</div>}
              <div className="modal-body">
                <label>
                  <span>{t("Paste accounts JSON")}</span>
                  <textarea
                    rows={6}
                    required
                    placeholder='[{"email":"user@gmail.com","refresh_token":"1//..."}]'
                    value={importJson}
                    onChange={(e) => setImportJson(e.target.value)}
                  />
                </label>
                <div>
                  <input
                    type="file"
                    accept=".json"
                    onChange={(e) => {
                      const file = e.target.files?.[0];
                      if (file) void handleImportFile(file);
                    }}
                    style={{ fontSize: 12 }}
                  />
                </div>
              </div>
              <div className="modal-actions">
                <button
                  type="button"
                  className="secondary-button"
                  onClick={() => setShowImportModal(false)}
                >
                  {t("Cancel")}
                </button>
                <button
                  type="submit"
                  className="primary-button"
                  disabled={importLoading}
                >
                  {importLoading ? t("Switching...") : t("Import")}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </section>
  );
}
