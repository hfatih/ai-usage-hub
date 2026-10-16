import { Fragment } from "react";
import { Activity, ArrowRight, Clock3, Pause, Play, ShieldCheck, Users } from "lucide-react";
import type { PageId } from "../components/AppShell";
import { ProviderCard } from "../components/ProviderCard";
import { api } from "../lib/ipc";
import { localizeValue, useTranslation } from "../i18n";
import { byProviderOrder, PROVIDER_INITIALS } from "../lib/providers";
import { formatDuration, formatUsageValue, relativeTime } from "../lib/format";
import { useHub } from "../stores/hub";
import { byPlatform } from "../lib/platform";

export function DashboardPage({ onNavigate }: { onNavigate: (page: PageId) => void }) {
  const { language, t } = useTranslation();
  const { snapshot, accounts, loading, refresh } = useHub();
  if (loading || !snapshot) return <DashboardSkeleton />;
  const providers = byProviderOrder(snapshot.providers, (provider) => provider.id);
  const groupedAccounts = byProviderOrder(accounts, (account) => account.providerId);
  const running = snapshot.applications.filter((app) => app.state === "running");
  const trackedToday = snapshot.applications.reduce((total, app) => total + app.todaySeconds, 0);

  async function toggleMonitoring() {
    await api.setMonitoringPaused(!snapshot!.monitoringPaused);
    await refresh();
  }

  return (
    <>
      <section className="hero-row">
        <div><span className="eyebrow">{t("Good to see you")}</span><h1>{t("Your AI workspace,")}<br /><em>{t("quietly organized.")}</em></h1><p>{t(byPlatform("Live app status and verified provider data — kept local on this PC.", "Live app status and verified provider data — kept local on this Mac."))}</p></div>
        <div className="hero-actions"><button className="secondary-button" onClick={() => void toggleMonitoring()}>{snapshot.monitoringPaused ? <Play size={16} /> : <Pause size={16} />}{t(snapshot.monitoringPaused ? "Resume monitoring" : "Pause monitoring")}</button></div>
      </section>

      <section className="summary-strip" aria-label={t("At a glance")}>
        <div><span className="summary-icon summary-icon--success"><Activity size={18} /></span><p><strong>{running.length}</strong><small>{t("Apps running now")}</small></p></div>
        <div><span className="summary-icon"><Clock3 size={18} /></span><p><strong>{formatDuration(trackedToday)}</strong><small>{t("Application-open time today")}</small></p></div>
        <div><span className="summary-icon summary-icon--purple"><ShieldCheck size={18} /></span><p><strong>{t("Local only")}</strong><small>{t("No telemetry enabled")}</small></p></div>
        <div className="summary-refresh"><small>{t("Last scan")}</small><strong>{relativeTime(snapshot.refreshedAt)}</strong></div>
      </section>

      <div className="section-heading"><div><span className="eyebrow">{t("Providers")}</span><h2>{t("Everything at a glance")}</h2></div><button className="text-button" onClick={() => onNavigate("integrations")}>{t("Manage integrations")} <ArrowRight size={15} /></button></div>
      <section className="provider-grid">
        {providers.map((provider) => <ProviderCard key={provider.id} provider={provider} apps={snapshot.applications.filter((app) => app.providerId === provider.id)} />)}
      </section>
      <div className="section-heading"><div><span className="eyebrow">{t("Accounts")}</span><h2>{t("Saved account usage")}</h2></div><button className="text-button" onClick={() => onNavigate("accounts")}>{t("Manage accounts")} <ArrowRight size={15} /></button></div>
      <section className="dashboard-account-list">
        {groupedAccounts.length ? groupedAccounts.map((account, index) => <Fragment key={account.id}>{groupedAccounts[index - 1]?.providerId !== account.providerId && <span className="eyebrow provider-group-title">{account.providerName}</span>}<button onClick={() => onNavigate("accounts")} className="dashboard-account-row"><span className={`provider-logo provider-logo--${account.providerId}`}>{PROVIDER_INITIALS[account.providerId] ?? account.providerName.slice(0, 1)}</span><div><strong>{account.email}</strong><small>{account.providerName} · {account.connectionStatus === "connected" ? t("Active") : t("Last known {time}", { time: relativeTime(account.lastRefresh) })}</small></div><div className="dashboard-account-metrics">{account.usage.slice(0, 4).map((usage) => <span key={usage.metric}><strong>{formatUsageValue(usage)}</strong><small>{localizeValue(usage.label, language)}</small></span>)}</div></button></Fragment>) : <div className="glass-panel empty-inline"><Users size={18} />{t("Accounts appear here after the first successful provider refresh.")}</div>}
      </section>
      <div className="privacy-footer"><ShieldCheck size={15} /><span>{t("Usage fields stay empty until a documented provider source is connected.")}</span></div>
    </>
  );
}

function DashboardSkeleton() {
  return <div className="skeleton-page"><span /><span /><div><span /><span /><span /><span /></div></div>;
}
