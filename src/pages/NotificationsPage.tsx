import { Bell, Save } from "lucide-react";
import { useEffect, useState } from "react";
import { relativeTime } from "../lib/format";
import { localizeEvent, useTranslation } from "../i18n";
import { useHub } from "../stores/hub";
import type { AppSettings } from "../types";

export function NotificationsPage() {
  const { language, t } = useTranslation();
  const { settings, notifications, saveSettings } = useHub();
  const [draft, setDraft] = useState<AppSettings | null>(settings);
  useEffect(() => setDraft(settings), [settings]);
  if (!draft) return null;
  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => setDraft({ ...draft, [key]: value });
  return <section><div className="page-heading"><div><span className="eyebrow">{t("Alert rules")}</span><h1>{t("Notifications")}</h1><p>{t("Local alerts are recorded when verified remaining-usage thresholds are crossed.")}</p></div><button className="primary-button" onClick={() => void saveSettings(draft)}><Save size={16} />{t("Save rules")}</button></div>
    <div className="settings-panel glass-panel">
      <div className="settings-section"><div><Bell size={19} /><span><strong>{t("Usage alerts")}</strong><small>{t("Enable local low-quota and reset alerts.")}</small></span></div><label className="switch"><input type="checkbox" checked={draft.notificationsEnabled} onChange={(event) => update("notificationsEnabled", event.target.checked)} /><span /></label></div>
      <div className="settings-section"><div><span><strong>{t("Desktop notifications")}</strong><small>{t("Show a Windows notification for each new alert, even when the window is hidden.")}</small></span></div><label className="switch"><input type="checkbox" checked={draft.desktopNotifications} onChange={(event) => update("desktopNotifications", event.target.checked)} /><span /></label></div>
      <div className="settings-section"><div><span><strong>{t("Low remaining threshold")}</strong><small>{t("Create a warning when remaining usage crosses this value.")}</small></span></div><label className="number-field"><input type="number" min="1" max="99" value={draft.notifyWarningPercent} onChange={(event) => update("notifyWarningPercent", Number(event.target.value))} /><span>%</span></label></div>
      <div className="settings-section"><div><span><strong>{t("Critical remaining threshold")}</strong><small>{t("Use a high-priority warning below this value.")}</small></span></div><label className="number-field"><input type="number" min="1" max={draft.notifyWarningPercent} value={draft.notifyCriticalPercent} onChange={(event) => update("notifyCriticalPercent", Number(event.target.value))} /><span>%</span></label></div>
      <div className="settings-section"><div><span><strong>{t("Reset alerts")}</strong><small>{t("Notify when a quota window jumps upward after reset.")}</small></span></div><label className="switch"><input type="checkbox" checked={draft.notifyOnReset} onChange={(event) => update("notifyOnReset", event.target.checked)} /><span /></label></div>
      <div className="settings-section"><div><span><strong>{t("Provider errors")}</strong><small>{t("Keep provider refresh failures visible.")}</small></span></div><label className="switch"><input type="checkbox" checked={draft.notifyOnProviderError} onChange={(event) => update("notifyOnProviderError", event.target.checked)} /><span /></label></div>
    </div>
    <div className="section-heading"><div><span className="eyebrow">{t("Inbox")}</span><h2>{t("Recent alerts")}</h2></div></div>
    <div className="glass-panel alert-list">{notifications.length ? notifications.map((item) => { const content = localizeEvent(item.title, item.detail, item.eventType, language); return <div className={`alert-row alert-row--${item.severity}`} key={item.id}><Bell size={15} /><div><strong>{content.title}</strong><small>{content.detail}</small></div><time>{relativeTime(item.createdAt)}</time></div>; }) : <div className="empty-state"><Bell size={25} /><strong>{t("No alerts")}</strong><span>{t("Alerts are created only for verified threshold crossings.")}</span></div>}</div>
  </section>;
}
