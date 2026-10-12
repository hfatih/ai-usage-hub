import { ArrowUpRight, CircleAlert, Clock3, ExternalLink, ShieldCheck } from "lucide-react";
import { api } from "../lib/ipc";
import { localizeValue, useTranslation } from "../i18n";
import { formatClock, formatDuration, formatUsageValue, relativeTime, sourceLabel, usageProgress, usageTiming } from "../lib/format";
import { PROVIDER_INITIALS } from "../lib/providers";
import type { AppStatus, ProviderSummary } from "../types";

export function ProviderCard({ provider, apps }: { provider: ProviderSummary; apps: AppStatus[] }) {
  const { language, t } = useTranslation();
  const mainApp = apps.find((app) => app.id === "chatgpt" && app.state === "running") ?? apps.find((app) => app.state === "running") ?? apps[0];
  const usage = provider.usage.filter((item) => item.value != null);
  const statusLabel = provider.usageAllowed === false
    ? "Limit reached"
    : provider.connectionStatus === "connected"
      ? "Connected"
      : provider.connectionStatus === "error"
        ? "Unavailable"
        : "Not connected";

  return (
    <article className={`provider-card provider-card--${provider.id}`}>
      <div className="card-head">
        <div className="provider-identity">
          <span className={`provider-logo provider-logo--${provider.id}`}>{PROVIDER_INITIALS[provider.id] ?? provider.name.slice(0, 1)}</span>
          <div><span className="provider-name">{provider.name}</span><strong className="account-email">{localizeValue(provider.accountLabel, language)}</strong>{provider.plan && <small className="plan-label">{localizeValue(provider.plan, language)}</small>}</div>
        </div>
        <span className={`connection-pill connection-pill--${provider.usageAllowed === false ? "limited" : provider.connectionStatus}`}>{t(statusLabel)}</span>
      </div>

      <div className="usage-block">
        {usage.length > 0 ? <div className="usage-metrics">{usage.map((item) => {
          const source = item.source;
          const percentage = usageProgress(item);
          return <div className="usage-metric" key={item.metric}>
            <div className="usage-label"><span>{localizeValue(item.label, language)}</span><span className={`source-badge source-badge--${source}`}>{source === "official_api" || source === "official_cli" ? <ShieldCheck size={12} /> : <CircleAlert size={12} />}{sourceLabel(source)}</span></div>
            <div className="usage-value">{formatUsageValue(item)}</div>
            {percentage != null && <div className="usage-track"><span style={{ width: `${percentage}%` }} /></div>}
            <div className="usage-reset"><Clock3 size={11} />{usageTiming(item)}</div>
          </div>;
        })}</div> : <div className="unavailable-value">{t("Unavailable")} <small>{provider.error ? localizeValue(provider.error, language) : t(provider.id === "openai" ? "Open Codex and sign in to read subscription usage" : provider.id === "anthropic" ? "Open Claude Desktop and sign in to refresh the shared Desktop / Code quota." : provider.id === "grok" && provider.connectionStatus === "connected" ? "Key accepted. xAI reports no balance for a regular API key." : ["kimi", "grok", "deepseek"].includes(provider.id) ? "Add an API key in Integrations to connect" : "No supported usage source connected")}</small></div>}
      </div>

      <div className="app-state-row">
        <div><span className={`state-dot state-dot--${mainApp?.state ?? "unknown"}`} /><div><strong>{mainApp?.displayName ?? t("No app configured")}</strong><small>{mainApp?.state === "running" ? `${t("Running")} · ${formatClock(mainApp.currentSessionSeconds)}` : mainApp?.lastUsedAt ? t("Last used {time}", { time: relativeTime(mainApp.lastUsedAt) }) : t("Not running")}</small></div></div>
        {mainApp && <span className="session-chip"><Clock3 size={13} />{formatDuration(mainApp.todaySeconds)} {t("today")}</span>}
      </div>

      <div className="card-footer">
        <span>{t("Refreshed {time}", { time: relativeTime(provider.lastRefresh) })}</span>
        <div>
          <button className="card-action" disabled={!mainApp?.launchable} title={mainApp?.launchable ? t("Open {name}", { name: mainApp.displayName }) : t("Set the executable in Apps")} onClick={() => mainApp && void api.launchApp(mainApp.id)}>{t("Open")} <ExternalLink size={14} /></button>
          <button className="round-action" aria-label={t("{name} details", { name: provider.name })}><ArrowUpRight size={15} /></button>
        </div>
      </div>
    </article>
  );
}
