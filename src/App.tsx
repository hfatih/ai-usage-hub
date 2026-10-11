import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import appConfig from "../app.config.json";
import { AppShell, type PageId } from "./components/AppShell";
import { SetupWizard } from "./components/SetupWizard";
import { ActivityPage } from "./pages/ActivityPage";
import { AppsPage } from "./pages/AppsPage";
import { DashboardPage } from "./pages/DashboardPage";
import { AccountsPage } from "./pages/AccountsPage";
import { ProxyPage } from "./pages/ProxyPage";
import { IntegrationsPage } from "./pages/IntegrationsPage";
import { NotificationsPage } from "./pages/NotificationsPage";
import { SecurityPage } from "./pages/SecurityPage";
import { SettingsPage } from "./pages/SettingsPage";
import { UsagePage } from "./pages/UsagePage";
import { useHub } from "./stores/hub";

const completedKey = "ai-usage-hub.setup-complete";

export default function App() {
  const [page, setPage] = useState<PageId>("dashboard");
  const [setupComplete, setSetupComplete] = useState(() => localStorage.getItem(completedKey) === "true");
  const { load, refresh, settings } = useHub();

  useEffect(() => {
    document.documentElement.lang = settings?.language === "tr" ? "tr" : "en";
  }, [settings?.language]);

  useEffect(() => {
    void load();
    if (!("__TAURI_INTERNALS__" in window)) return;
    const statusListener = listen("hub://status-changed", () => void load());
    const navigationListener = listen<string>("hub://navigate", (event) => setPage(event.payload as PageId));
    return () => {
      void statusListener.then((unlisten) => unlisten());
      void navigationListener.then((unlisten) => unlisten());
    };
  }, [load, refresh]);

  if (!setupComplete) {
    return (
      <SetupWizard
        appName={appConfig.name}
        tagline={appConfig.tagline}
        onComplete={() => {
          localStorage.setItem(completedKey, "true");
          setSetupComplete(true);
        }}
      />
    );
  }

  const content = {
    dashboard: <DashboardPage onNavigate={setPage} />,
    accounts: <AccountsPage />,
    proxy: <ProxyPage />,
    apps: <AppsPage />,
    usage: <UsagePage />,
    activity: <ActivityPage />,
    notifications: <NotificationsPage />,
    integrations: <IntegrationsPage onNavigate={setPage} />,
    security: <SecurityPage />,
    settings: <SettingsPage />,
  }[page];

  return <AppShell page={page} onPageChange={setPage}>{content}</AppShell>;
}
