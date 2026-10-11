import type { ReactNode } from "react";
import {
  Activity,
  Bell,
  Boxes,
  ChartNoAxesCombined,
  CircleUserRound,
  Gauge,
  LayoutGrid,
  RefreshCw,
  Server,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  UserRoundCog,
} from "lucide-react";
import { useHub } from "../stores/hub";
import { localizeValue, useTranslation } from "../i18n";

export type PageId = "dashboard" | "accounts" | "proxy" | "apps" | "usage" | "activity" | "notifications" | "integrations" | "security" | "settings";

const navItems: Array<{ id: PageId; label: string; icon: typeof Gauge }> = [
  { id: "dashboard", label: "Dashboard", icon: LayoutGrid },
  { id: "accounts", label: "Accounts", icon: UserRoundCog },
  { id: "proxy", label: "API Proxy", icon: Server },
  { id: "apps", label: "Apps", icon: Boxes },
  { id: "usage", label: "Usage", icon: ChartNoAxesCombined },
  { id: "activity", label: "Activity", icon: Activity },
  { id: "notifications", label: "Notifications", icon: Bell },
  { id: "integrations", label: "Integrations", icon: SlidersHorizontal },
  { id: "security", label: "Security", icon: ShieldCheck },
  { id: "settings", label: "Settings", icon: Settings },
];

interface Props {
  page: PageId;
  onPageChange: (page: PageId) => void;
  children: ReactNode;
}

export function AppShell({ page, onPageChange, children }: Props) {
  const { language, t } = useTranslation();
  const { refresh, refreshing, error, snapshot } = useHub();
  const running = snapshot?.applications.filter((app) => app.state === "running").length ?? 0;

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <nav aria-label={t("Control center")}>
          {navItems.map(({ id, label, icon: Icon }) => (
            <button key={id} className={`nav-item${page === id ? " is-active" : ""}`} onClick={() => onPageChange(id)}>
              <Icon size={18} strokeWidth={1.8} />
              <span>{t(label)}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar-status">
          <span className={`pulse-dot${snapshot?.monitoringPaused ? " is-paused" : ""}`} />
          <div><strong>{t(snapshot?.monitoringPaused ? "Monitoring paused" : "Monitoring active")}</strong><small>{t(running === 1 ? "{count} app running" : "{count} apps running", { count: running })}</small></div>
        </div>
      </aside>

      <main className="main-panel">
        <header className="topbar">
          <div className="topbar-title">
            <span className="eyebrow">{t("Control center")}</span>
            <span>{t(navItems.find((item) => item.id === page)?.label ?? "Dashboard")}</span>
          </div>
          <div className="topbar-actions">
            <button className="icon-button" aria-label={t("Refresh all")} title={t("Refresh all")} onClick={() => void refresh()} disabled={refreshing}>
              <RefreshCw size={18} className={refreshing ? "spin" : ""} />
            </button>
            <button className="icon-button" aria-label={t("Notifications")} title={t("Notifications")} onClick={() => onPageChange("notifications")}><Bell size={18} /></button>
            <button className="icon-button" aria-label={t("Settings")} title={t("Settings")} onClick={() => onPageChange("settings")}><Settings size={18} /></button>
            <button className="avatar-button" aria-label={t("Local profile")}><CircleUserRound size={19} /><span>{t("Local")}</span></button>
          </div>
        </header>
        {error && <div className="error-banner" role="alert">{localizeValue(error, language)}</div>}
        <div className="page-content">{children}</div>
      </main>
    </div>
  );
}
