import { ChartNoAxesCombined, Clock3 } from "lucide-react";
import { localizeValue, useTranslation } from "../i18n";
import { formatUsageValue, isPercentageUsage, relativeTime, sourceLabel, usageProgress, usageTiming } from "../lib/format";
import { useHub } from "../stores/hub";
import { Sparkline } from "../components/Sparkline";

export function UsagePage() {
  const { language, t } = useTranslation();
  const { snapshot, accounts, usageHistory, usageTrends } = useHub();
  const metrics = [
    ...accounts.flatMap((account) => account.usage.map((usage) => ({ id: account.id, providerName: account.providerName, email: account.email, usage }))),
    ...(snapshot?.providers.flatMap((provider) => provider.usage.filter((usage) => usage.source === "local_logs" && usage.value != null).map((usage) => ({ id: `device:${provider.id}`, providerName: provider.name, email: t("This device · all accounts"), usage }))) ?? []),
  ];
  return (
    <section>
      <div className="page-heading"><div><span className="eyebrow">{t("Verified usage")}</span><h1>{t("Usage")}</h1><p>{t("Percentages come from official provider or local application sources; history is stored per account.")}</p></div></div>
      <div className="usage-summary-grid">{metrics.map(({ id, providerName, email, usage }) => {
        const percent = usageProgress(usage);
        return <article className="glass-panel usage-summary-card" key={`${id}:${usage.metric}`}><div><span>{providerName}</span><small>{email}</small></div><strong>{formatUsageValue(usage)}{isPercentageUsage(usage) && <small> {t("remaining")}</small>}</strong><p>{localizeValue(usage.label, language)}</p>{percent != null && <div className="usage-track"><span style={{ width: `${percent}%` }} /></div>}<Sparkline trend={usageTrends.find((trend) => trend.accountId === id && trend.metric === usage.metric)} current={usage.value} percent={isPercentageUsage(usage)} label={t("Last 24 hours")} /><footer>{sourceLabel(usage.source)} · {usageTiming(usage)}</footer></article>;
      })}</div>
      <div className="section-heading"><div><span className="eyebrow">{t("History")}</span><h2>{t("Recent samples")}</h2></div><span className="section-count">{t(usageHistory.length === 1 ? "{count} sample" : "{count} samples", { count: usageHistory.length })}</span></div>
      <div className="glass-panel usage-history">
        {usageHistory.length === 0 ? <div className="empty-state"><ChartNoAxesCombined size={28} /><strong>{t("No usage samples yet")}</strong><span>{t("Refresh a connected provider to record a verified sample.")}</span></div> : usageHistory.map((point) => <div className="usage-history-row" key={point.id}><span className={`provider-logo provider-logo--${point.providerId}`}>{point.providerName.slice(0, 1)}</span><div><strong>{localizeValue(point.usage.label, language)}</strong><small>{point.accountLabel} · {point.providerName}</small></div><span>{formatUsageValue(point.usage)}</span><time><Clock3 size={11} />{relativeTime(point.usage.retrievedAt)}</time></div>)}
      </div>
    </section>
  );
}
