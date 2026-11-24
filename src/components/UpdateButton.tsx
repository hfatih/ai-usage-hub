import { Download, ExternalLink, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { version } from "../../package.json";
import { useTranslation } from "../i18n";
import { api, type UpdateInfo } from "../lib/ipc";

const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

export function UpdateButton({ settings = false }: { settings?: boolean }) {
  const { t } = useTranslation();
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState<"check" | "install" | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    if (settings) return;
    const check = () => api.checkForUpdate().then(setUpdate).catch(() => undefined);
    void check();
    const timer = window.setInterval(check, CHECK_EVERY_MS);
    return () => window.clearInterval(timer);
  }, [settings]);

  async function check() {
    setBusy("check");
    setError("");
    try {
      setUpdate(await api.checkForUpdate());
    } catch {
      setUpdate(null);
      setError(t("Could not check GitHub. Check your connection and try again."));
    } finally {
      setBusy(null);
    }
  }

  async function install() {
    if (!update?.canInstall || !window.confirm(t("Install AI Usage Hub {version}? The app closes while the installer runs.", { version: update.latestVersion }))) return;
    setBusy("install");
    setError("");
    try {
      await api.installUpdate(update.latestVersion);
    } catch (failure) {
      if (settings) {
        setUpdate(null);
        setError(t("The update could not be installed. Check for updates and try again."));
      } else {
        window.alert(String(failure));
      }
    } finally {
      setBusy(null);
    }
  }

  async function openReleases() {
    try {
      await api.openUpdateReleases();
    } catch {
      setError(t("Could not open GitHub Releases."));
    }
  }

  const installButton = update?.available && update.canInstall && (
    <button type="button" className="primary-button update-button" onClick={() => void install()} disabled={busy !== null}>
      <Download size={14} className={busy === "install" ? "spin" : ""} />
      {busy === "install" ? t("Downloading update...") : t("Update to {version}", { version: update.latestVersion })}
    </button>
  );

  if (!settings) return installButton || null;

  let status = t("Check for a new stable version on GitHub.");
  if (busy === "check") status = t("Checking GitHub...");
  else if (update?.available) status = update.canInstall
    ? t("Version {version} is available on GitHub", { version: update.latestVersion })
    : t("Open GitHub Releases to see the installation options.");
  else if (update) status = update.currentVersion === update.latestVersion
    ? t("You are up to date.")
    : t("Your installed version is newer than the GitHub release.");

  return (
    <>
      <div className="settings-group-title">{t("Software updates")}</div>
      <div className="settings-section update-settings">
        <div className="update-settings-info">
          <Download size={19} />
          <span>
            <strong>{t("Installed version {version}", { version: update?.currentVersion ?? version })}</strong>
            {update && <small>{t("Latest GitHub release {version}", { version: update.latestVersion })}</small>}
            <small role={error ? "alert" : "status"}>{error || status}</small>
          </span>
        </div>
        <div className="update-settings-actions">
          <button type="button" className="secondary-button" onClick={() => void openReleases()} disabled={busy !== null}>
            <ExternalLink size={14} /> {t("GitHub Releases")}
          </button>
          <button type="button" className="secondary-button" onClick={() => void check()} disabled={busy !== null}>
            <RefreshCw size={14} className={busy === "check" ? "spin" : ""} /> {t("Check for updates")}
          </button>
          {installButton}
        </div>
      </div>
    </>
  );
}
