import { Boxes, CheckCircle2, CircleOff, ExternalLink, FolderSearch, ShieldCheck } from "lucide-react";
import type { PageId } from "../components/AppShell";
import { api } from "../lib/ipc";
import { localizeValue, useTranslation } from "../i18n";
import { useHub } from "../stores/hub";

const setupText: Record<string, string> = {
  openai: "Open Codex and sign in with your ChatGPT account. The 5-hour and weekly windows are read automatically.",
  anthropic: "Sign in to Claude Desktop or Claude Code. Their shared 5-hour and weekly quota is read from local Claude history.",
  google: "Open Antigravity and sign in with your Google account. Gemini and third-party model quotas are read from the local app.",
  opencode: "Connect a provider in OpenCode. Model providers, the free catalog and local token/cost totals are read separately.",
  kimi: "The Kimi app is detected; no verified local source provides a quota percentage.",
  github: "VS Code is detected; no supported official local source provides Copilot subscription quotas.",
};

export function IntegrationsPage({ onNavigate }: { onNavigate: (page: PageId) => void }) {
  const { language, t } = useTranslation();
  const { snapshot, accounts, refresh } = useHub();
  if (!snapshot) return null;

  async function browse(appId: string) {
    const path = await api.chooseExecutable(t("Windows applications"));
    if (!path) return;
    await api.setExecutable(appId, path);
    await refresh();
  }

  return <section>
    <div className="page-heading"><div><span className="eyebrow">{t("Integration center")}</span><h1>{t("Integrations")}</h1><p>{t("Real accounts, quota sources, application links and setup status in one place.")}</p></div></div>
    <div className="integration-grid">{snapshot.providers.map((provider) => {
      const apps = snapshot.applications.filter((app) => app.providerId === provider.id);
      const saved = accounts.filter((account) => account.providerId === provider.id);
      const currentUsage = provider.usage.filter((usage) => usage.value != null && usage.source !== "estimated");
      const savedUsage = (saved[0]?.usage ?? []).filter((usage) => usage.value != null && usage.source !== "estimated");
      const verifiedUsage = currentUsage.length ? currentUsage : savedUsage;
      const usingSavedUsage = currentUsage.length === 0 && savedUsage.length > 0;
      const connected = provider.connectionStatus === "connected";
      const hasExactReset = currentUsage.some((usage) => usage.resetAt != null && usage.source !== "locally_calculated");
      const hasSavedReset = usingSavedUsage && savedUsage.some((usage) => usage.resetAt != null);
      const hasEstimatedReset = currentUsage.some((usage) => usage.resetAt != null && usage.source === "locally_calculated");
      const hasTokens = verifiedUsage.some((usage) => usage.unit === "tokens");
      const hasCost = verifiedUsage.some((usage) => usage.unit === "USD");
      const trackingLabel = hasTokens && hasCost ? "Local token + cost totals" : hasTokens ? "Local tokens; billed cost unavailable" : hasCost ? "Local cost totals" : "Token / cost not supplied";
      return <article className="glass-panel integration-card" key={provider.id}>
        <header><span className={`provider-logo provider-logo--${provider.id}`}>{provider.name.slice(0, 1)}</span><div><small>{provider.name}</small><strong>{localizeValue(provider.accountLabel, language)}</strong><span>{provider.plan ? localizeValue(provider.plan, language) : t(saved.length === 1 ? "{count} saved account" : "{count} saved accounts", { count: saved.length })}</span></div><span className={`integration-status integration-status--${provider.connectionStatus}`}>{connected ? <CheckCircle2 size={14} /> : <CircleOff size={14} />}{t(connected ? "Connected" : provider.connectionStatus === "error" ? "Unavailable" : "Setup needed")}</span></header>
        <p className="integration-setup">{provider.id === "google" && provider.error ? t("Open Antigravity and refresh for live quotas. Saved readings can be old.") : provider.error ? localizeValue(provider.error, language) : t(setupText[provider.id] ?? "No supported connection source is available for this provider yet.")}</p>
        <div className="capability-list">
          <Capability enabled={verifiedUsage.length > 0} label={t(usingSavedUsage ? "{count} last known usage metrics" : verifiedUsage.length === 1 ? "{count} verified usage metric" : "{count} verified usage metrics", { count: verifiedUsage.length })} />
          <Capability enabled={hasExactReset || hasEstimatedReset || hasSavedReset} label={t(hasExactReset ? "Exact reset times" : hasEstimatedReset ? "Estimated reset times" : hasSavedReset ? "Last known reset times" : "Reset time not supplied")} />
          <Capability enabled={saved.length > 0} label={t(saved.length === 1 ? "{count} saved account" : "{count} saved accounts", { count: saved.length })} />
          <Capability enabled={apps.length > 0 && provider.capabilities.localSessionDetection} label={t("Local app detection")} />
          <Capability enabled={hasTokens || hasCost} label={t(trackingLabel)} />
          <Capability enabled={apps.some((app) => app.launchable)} label={t("Application launch ready")} />
        </div>
        <div className="integration-apps">
          {apps.length ? apps.map((app) => <div key={app.id}><span className={`state-dot state-dot--${app.state}`} /><p><strong>{app.displayName}</strong><small>{t(app.state === "running" ? "Running now" : app.executablePath ? "Ready to launch" : "Executable not configured")}</small></p><button className="icon-button" title={t("Choose {name} executable", { name: app.displayName })} onClick={() => void browse(app.id)}><FolderSearch size={14} /></button><button className="icon-button" title={t("Open {name}", { name: app.displayName })} disabled={!app.launchable} onClick={() => void api.launchApp(app.id)}><ExternalLink size={14} /></button></div>) : <span className="integration-no-app"><Boxes size={13} />{t("No linked application")}</span>}
        </div>
      </article>;
    })}</div>
    <p className="page-note"><ShieldCheck size={13} />{t("Provider credentials are not copied. Supported official CLI, local app surfaces or read-only local usage data are used.")} <button className="text-button" onClick={() => onNavigate("apps")}>{t("Manage application paths")}</button></p>
  </section>;
}

function Capability({ enabled, label }: { enabled: boolean; label: string }) {
  return <span className={enabled ? "yes" : ""}>{enabled ? <CheckCircle2 size={12} /> : <CircleOff size={12} />}{label}</span>;
}
