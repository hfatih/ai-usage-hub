import { ExternalLink, FolderSearch, MonitorDot } from "lucide-react";
import { api } from "../lib/ipc";
import { useTranslation } from "../i18n";
import { formatClock, formatDuration, relativeTime } from "../lib/format";
import { useHub } from "../stores/hub";

export function AppsPage() {
  const { t } = useTranslation();
  const { snapshot, refresh } = useHub();

  async function browse(appId: string) {
    const path = await api.chooseExecutable(t("Windows applications"));
    if (!path) return;
    await api.setExecutable(appId, path);
    await refresh();
  }

  return (
    <section>
      <div className="page-heading"><div><span className="eyebrow">{t("Local applications")}</span><h1>{t("Apps")}</h1><p>{t("Process detection tracks application-open time, never token consumption.")}</p></div></div>
      <div className="apps-table glass-panel">
        <div className="apps-table-head"><span>{t("Application")}</span><span>{t("Status")}</span><span>{t("Today")}</span><span>{t("Last used")}</span><span>{t("Executable")}</span><span /></div>
        {snapshot?.applications.map((app) => (
          <div className="app-table-row" key={app.id}>
            <div className="app-title"><span className={`app-orb app-orb--${app.providerId ?? "local"}`}>{app.displayName.slice(0, 1)}</span><div><strong>{app.displayName}</strong><small>{app.detectedExecutable ?? app.providerId ?? t("Local app")}</small></div></div>
            <span className={`status-label status-label--${app.state}`}><i />{app.state === "running" ? t("Running {time}", { time: formatClock(app.currentSessionSeconds) }) : t(app.state === "not_running" ? "Not running" : "Unknown")}</span>
            <span>{formatDuration(app.todaySeconds)}</span>
            <span>{relativeTime(app.lastUsedAt)}</span>
            <span className="path-label" title={app.executablePath ?? t("Not configured")}>{app.executablePath ?? t("Not configured")}</span>
            <div className="row-actions"><button className="icon-button" title={t("Choose executable")} onClick={() => void browse(app.id)}><FolderSearch size={16} /></button><button className="icon-button" title={t("Open {name}", { name: app.displayName })} disabled={!app.launchable} onClick={() => void api.launchApp(app.id)}><ExternalLink size={16} /></button></div>
          </div>
        ))}
        {!snapshot?.applications.length && <div className="empty-state"><MonitorDot size={28} /><strong>{t("No applications configured")}</strong></div>}
      </div>
    </section>
  );
}
