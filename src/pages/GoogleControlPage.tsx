import { useCallback, useEffect, useRef, useState } from "react";
import {
  Activity as ActivityIcon,
  CheckCircle2,
  Cloud,
  FolderOpen,
  Gauge,
  LayoutDashboard,
  Plus,
  RefreshCw,
  Search,
  Settings as SettingsIcon,
  ShieldCheck,
  Users,
  X,
} from "lucide-react";
import { googleApi, type GoogleSnapshot } from "../lib/googleControl";
import { ControlDialog, type Dialog } from "./google/ControlDialog";
import { Accounts } from "./google/Accounts";
import { Activity } from "./google/Activity";
import { Overview } from "./google/Overview";
import { Projects } from "./google/Projects";
import { Services } from "./google/Services";
import { Settings } from "./google/Settings";
import { Usage } from "./google/Usage";
import { useGoogleText, type Tab } from "./google/shared";
import "../styles/google-control.css";

export function GoogleControlPage({
  initialSearch = "",
}: { initialSearch?: string } = {}) {
  const text = useGoogleText();
  const [snapshot, setSnapshot] = useState<GoogleSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<Tab>(initialSearch ? "accounts" : "overview");
  const [query, setQuery] = useState(initialSearch);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [searchRequest, setSearchRequest] = useState(0);
  const searchRef = useRef<HTMLInputElement>(null);
  const mounted = useRef(true);
  const pending = useRef(false);
  useEffect(() => {
    mounted.current = true;
    googleApi
      .snapshot()
      .then((value) => {
        if (mounted.current) setSnapshot(value);
      })
      .catch((err) => {
        if (mounted.current) setError(String(err));
      })
      .finally(() => {
        if (mounted.current) setLoading(false);
      });
    return () => {
      mounted.current = false;
    };
  }, []);
  const run = useCallback(
    async (
      action: () => Promise<GoogleSnapshot>,
      message?: string,
    ): Promise<boolean> => {
      if (pending.current) return false;
      pending.current = true;
      setBusy(true);
      setError(null);
      setNotice(null);
      try {
        const value = await action();
        if (mounted.current) {
          setSnapshot(value);
          if (message) setNotice(message);
        }
        return true;
      } catch (err) {
        if (mounted.current) setError(String(err));
        return false;
      } finally {
        pending.current = false;
        if (mounted.current) setBusy(false);
      }
    },
    [],
  );
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(null), 6500);
    return () => clearTimeout(timer);
  }, [notice]);
  useEffect(() => {
    if (tab === "accounts" && searchRequest > 0) {
      searchRef.current?.focus();
      searchRef.current?.select();
    }
  }, [tab, searchRequest]);
  useEffect(() => {
    if (initialSearch) {
      setQuery(initialSearch);
      setTab("accounts");
    }
  }, [initialSearch]);
  const openAccount = () => {
    setError(null);
    setDialog({ kind: "account", account: null });
  };
  function focusSearch() {
    if (dialog) return;
    setTab("accounts");
    setSearchRequest((n) => n + 1);
  }
  useEffect(() => {
    function handleKey(event: KeyboardEvent) {
      const editing =
        event.target instanceof HTMLElement &&
        (event.target.matches("input,textarea,select") ||
          event.target.isContentEditable);
      if (event.key === "Escape" && !busy) {
        setDialog(null);
        return;
      }
      if (
        ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") ||
        (event.key === "/" && !editing)
      ) {
        if (dialog) return;
        event.preventDefault();
        setTab("accounts");
        setSearchRequest((n) => n + 1);
      }
      if (
        (event.ctrlKey || event.metaKey) &&
        event.key.toLowerCase() === "n" &&
        !busy &&
        !dialog
      ) {
        event.preventDefault();
        setError(null);
        setDialog({ kind: "account", account: null });
      }
    }
    document.addEventListener("keydown", handleKey);
    return () => document.removeEventListener("keydown", handleKey);
  }, [busy, dialog]);
  function restore(path: string) {
    setError(null);
    setDialog({
      kind: "confirm",
      title: text(
        "Restore ALL Hub data?",
        "TÜM Hub verileri geri yüklensin mi?",
      ),
      message: text(
        "This replaces the entire Hub database: all providers, accounts, projects, usage history and settings. A safety backup is created automatically before restore. Browser profiles remain separate.",
        "Bu işlem tüm Hub veritabanını değiştirir: bütün sağlayıcılar, hesaplar, projeler, kullanım geçmişi ve ayarlar. Geri yüklemeden önce otomatik güvenlik yedeği oluşturulur. Tarayıcı profilleri ayrı kalır.",
      ),
      detail: path,
      label: text("Restore all Hub data", "Tüm Hub verilerini geri yükle"),
      action: () => googleApi.restoreBackup(path),
    });
  }
  const tabs = [
    {
      id: "overview",
      label: text("Overview", "Genel bakış"),
      icon: LayoutDashboard,
    },
    { id: "accounts", label: text("Accounts", "Hesaplar"), icon: Users },
    { id: "services", label: text("Services", "Hizmetler"), icon: Cloud },
    { id: "projects", label: text("Projects", "Projeler"), icon: FolderOpen },
    {
      id: "usage",
      label: text("Usage & quotas", "Kullanım ve kotalar"),
      icon: Gauge,
    },
    { id: "activity", label: text("Activity", "Etkinlik"), icon: ActivityIcon },
    { id: "settings", label: text("Settings", "Ayarlar"), icon: SettingsIcon },
  ] as const;
  return (
    <div className="google-control" aria-busy={loading || busy}>
      <section className="page-heading">
        <div>
          <span className="eyebrow">GOOGLE & ANTIGRAVITY</span>
          <h1>
            {text("Your Google control center", "Google kontrol merkezin")}
          </h1>
          <p>
            {text(
              "Independent accounts. Their own sessions, subscriptions and quotas. One organized workspace.",
              "Bağımsız hesaplar. Her birinin kendi oturumu, aboneliği ve kotası. Tek düzenli çalışma alanı.",
            )}
          </p>
        </div>
        <div className="gc-heading-actions">
          <button
            className="icon-button"
            aria-label={text(
              "Search accounts (Ctrl K)",
              "Hesaplarda ara (Ctrl K)",
            )}
            onClick={focusSearch}
            disabled={loading}
          >
            <Search size={17} />
          </button>
          <button
            className="icon-button"
            aria-label={text(
              "Refresh Google control data",
              "Google kontrol verilerini yenile",
            )}
            disabled={loading || busy}
            onClick={() =>
              void run(
                () => googleApi.snapshot(),
                text(
                  "Management data refreshed.",
                  "Yönetim bilgileri yenilendi.",
                ),
              )
            }
          >
            <RefreshCw size={17} className={busy ? "gc-spin" : ""} />
          </button>
          <button
            className="primary-button"
            disabled={loading || busy || !snapshot}
            onClick={openAccount}
          >
            <Plus size={15} />
            {text("Add account", "Hesap ekle")}
          </button>
        </div>
      </section>
      <div
        className="gc-local-tabs"
        role="tablist"
        aria-label={text("Google control sections", "Google kontrol bölümleri")}
      >
        {tabs.map(({ id, label, icon: Icon }, index) => (
          <button
            key={id}
            id={`gc-tab-${id}`}
            role="tab"
            aria-selected={tab === id}
            aria-controls={`gc-panel-${id}`}
            tabIndex={tab === id ? 0 : -1}
            className={tab === id ? "is-active" : ""}
            onClick={() => setTab(id)}
            onKeyDown={(event) => {
              if (
                !["ArrowRight", "ArrowLeft", "Home", "End"].includes(event.key)
              )
                return;
              event.preventDefault();
              const next =
                event.key === "Home"
                  ? 0
                  : event.key === "End"
                    ? tabs.length - 1
                    : (index +
                        (event.key === "ArrowRight" ? 1 : -1) +
                        tabs.length) %
                      tabs.length;
              setTab(tabs[next].id);
              document.getElementById(`gc-tab-${tabs[next].id}`)?.focus();
            }}
          >
            <Icon size={15} />
            {label}
            {id === "accounts" && snapshot && (
              <span>{snapshot.accounts.filter((a) => !a.archived).length}</span>
            )}
          </button>
        ))}
      </div>
      {error && !dialog && (
        <div className="gc-notice gc-notice--error" role="alert">
          <span>{error}</span>
          <button
            className="icon-button"
            aria-label={text("Dismiss error", "Hatayı kapat")}
            onClick={() => setError(null)}
          >
            <X size={15} />
          </button>
        </div>
      )}
      {notice && (
        <div className="gc-notice gc-notice--success" role="status">
          <CheckCircle2 size={17} />
          <span>{notice}</span>
          <button
            className="icon-button"
            aria-label={text("Dismiss notification", "Bildirimi kapat")}
            onClick={() => setNotice(null)}
          >
            <X size={15} />
          </button>
        </div>
      )}
      {loading ? (
        <div
          className="gc-loading"
          role="status"
          aria-label={text(
            "Loading Google control center",
            "Google kontrol merkezi yükleniyor",
          )}
        >
          <div />
          <div />
          <div />
          <div />
        </div>
      ) : !snapshot ? (
        <div className="glass-panel gc-empty">
          <p>
            {text(
              "Google control data could not be loaded. Retry to connect to the local database.",
              "Google kontrol verileri yüklenemedi. Yerel veritabanına bağlanmak için yeniden dene.",
            )}
          </p>
          <button
            className="secondary-button"
            disabled={busy}
            onClick={() => void run(() => googleApi.snapshot())}
          >
            <RefreshCw size={15} />
            {text("Retry", "Yeniden dene")}
          </button>
        </div>
      ) : (
        <section
          role="tabpanel"
          id={`gc-panel-${tab}`}
          aria-labelledby={`gc-tab-${tab}`}
        >
          {tab === "overview" && (
            <Overview
              snapshot={snapshot}
              busy={busy}
              run={run}
              onTab={setTab}
            />
          )}
          {tab === "accounts" && (
            <Accounts
              snapshot={snapshot}
              query={query}
              setQuery={setQuery}
              searchRef={searchRef}
              onAdd={openAccount}
              busy={busy}
              run={run}
              onEdit={(account) => {
                setError(null);
                setDialog({ kind: "account", account });
              }}
              onArchive={(account) => {
                setError(null);
                setDialog({
                  kind: "confirm",
                  title: text(
                    "Archive management entry?",
                    "Yönetim kaydı arşivlensin mi?",
                  ),
                  message: text(
                    `Archive ${account.name || account.email} in the Google control center? Its underlying Antigravity sign-in and browser profile are kept. You can restore the management entry from Archived → Edit.`,
                    `${account.name || account.email} Google kontrol merkezinde arşivlensin mi? Mevcut Antigravity girişi ve tarayıcı profili korunur. Yönetim kaydını Arşiv → Düzenle üzerinden geri alabilirsin.`,
                  ),
                  label: text("Archive entry", "Kaydı arşivle"),
                  action: () => googleApi.removeAccount(account.id),
                });
              }}
            />
          )}
          {tab === "services" && (
            <Services snapshot={snapshot} busy={busy} run={run} />
          )}
          {tab === "projects" && (
            <Projects
              snapshot={snapshot}
              busy={busy}
              onAdd={() => {
                setError(null);
                setDialog({ kind: "project", project: null });
              }}
              onEdit={(project) => {
                setError(null);
                setDialog({ kind: "project", project });
              }}
              onDelete={(project) => {
                setError(null);
                setDialog({
                  kind: "confirm",
                  title: text("Delete project?", "Proje silinsin mi?"),
                  message: text(
                    `Delete “${project.name}” and its organizational assignments? Your Google accounts and service content remain separate.`,
                    `“${project.name}” ve düzenleme amaçlı atamaları silinsin mi? Google hesapların ve hizmet içerikleri ayrı kalır.`,
                  ),
                  label: text("Delete project", "Projeyi sil"),
                  action: () => googleApi.deleteProject(project.id),
                });
              }}
            />
          )}
          {tab === "usage" && (
            <Usage
              snapshot={snapshot}
              busy={busy}
              run={run}
              onDelete={(id) => {
                setError(null);
                setDialog({
                  kind: "confirm",
                  title: text(
                    "Delete manual measurement?",
                    "Manuel ölçüm silinsin mi?",
                  ),
                  message: text(
                    "This removes this measurement from local history. The service's actual usage is unchanged.",
                    "Bu ölçüm yerel geçmişten kaldırılır. Hizmetin gerçek kullanımı değişmez.",
                  ),
                  label: text("Delete measurement", "Ölçümü sil"),
                  action: () => googleApi.deleteUsage(id),
                });
              }}
            />
          )}
          {tab === "activity" && <Activity snapshot={snapshot} />}
          {tab === "settings" && (
            <Settings
              snapshot={snapshot}
              busy={busy}
              run={run}
              onRestore={restore}
              notify={(message, failure) => {
                if (failure) setError(message);
                else setNotice(message);
              }}
            />
          )}
        </section>
      )}
      <div className="privacy-footer">
        <ShieldCheck size={14} />
        <span>
          {text(
            "Google credentials stay in each browser. Management metadata stays local.",
            "Google kimlik bilgileri her hesabın tarayıcısında kalır. Yönetim bilgileri yerelde tutulur.",
          )}
        </span>
      </div>
      <ControlDialog
        dialog={dialog}
        snapshot={snapshot}
        busy={busy}
        error={error}
        run={run}
        onClose={() => setDialog(null)}
      />
    </div>
  );
}

export { GoogleSummary } from "./google/GoogleSummary";
export default GoogleControlPage;
