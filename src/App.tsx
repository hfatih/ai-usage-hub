import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import appConfig from "../app.config.json";
import { AppShell, type PageId } from "./components/AppShell";
import { SetupWizard } from "./components/SetupWizard";
import { ActivityPage } from "./pages/ActivityPage";
import { AppsPage } from "./pages/AppsPage";
import { DashboardPage } from "./pages/DashboardPage";
import { GoogleControlPage } from "./pages/GoogleControlPage";
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
  const [googleSearch, setGoogleSearch] = useState("");
  const [setupComplete, setSetupComplete] = useState(() => localStorage.getItem(completedKey) === "true");
  const { load, refresh, settings } = useHub();

  useEffect(() => {
    document.documentElement.lang = settings?.language === "tr" ? "tr" : "en";
  }, [settings?.language]);

  useEffect(() => {
    void load();
    const restored = () => { void refresh().then(() => load()); };
    window.addEventListener("google-control-restored", restored);
    if (!("__TAURI_INTERNALS__" in window)) return () => window.removeEventListener("google-control-restored", restored);
    const statusListener = listen("hub://status-changed", () => void load());
    const navigationListener = listen<string>("hub://navigate", (event) => setPage(event.payload as PageId));
    return () => {
      window.removeEventListener("google-control-restored", restored);
      void statusListener.then((unlisten) => unlisten());
      void navigationListener.then((unlisten) => unlisten());
    };
  }, [load, refresh]);

  useEffect(() => {
    const preference = settings?.theme ?? "dark";
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => {
      document.documentElement.dataset.theme = preference === "system" ? (media.matches ? "light" : "dark") : preference;
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [settings?.theme]);

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

  function navigate(target: PageId) {
    if (target === "google") setGoogleSearch("");
    setPage(target);
  }

  const content = {
    dashboard: <DashboardPage onNavigate={navigate} />,
    accounts: <AccountsPage onManageGoogle={(email) => { setGoogleSearch(email ?? ""); setPage("google"); }} />,
    google: <GoogleControlPage initialSearch={googleSearch} />,
    proxy: <ProxyPage />,
    apps: <AppsPage />,
    usage: <UsagePage />,
    activity: <ActivityPage />,
    notifications: <NotificationsPage />,
    integrations: <IntegrationsPage onNavigate={setPage} />,
    security: <SecurityPage />,
    settings: <SettingsPage />,
  }[page];

  return <AppShell page={page} onPageChange={navigate}>{content}</AppShell>;
}
