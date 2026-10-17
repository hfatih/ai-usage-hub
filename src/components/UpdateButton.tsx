import { Download } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "../i18n";
import { api, type UpdateInfo } from "../lib/ipc";
import { byPlatform } from "../lib/platform";

const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

/**
 * Shown in the top bar only when GitHub has a newer release. Installing downloads the setup EXE,
 * checks it against the release's SHA256SUMS.txt and runs it; the app then closes for the
 * installer (on macOS, where an update is offered only when the release has a .dmg, the release
 * page opens instead).
 */
export function UpdateButton() {
  const { t } = useTranslation();
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    const check = () => api.checkForUpdate().then(setUpdate).catch(() => undefined); // offline: try again later
    void check();
    const timer = window.setInterval(check, CHECK_EVERY_MS);
    return () => window.clearInterval(timer);
  }, []);

  if (!update?.available) return null;

  async function install() {
    if (!update || !window.confirm(t(byPlatform("Install AI Usage Hub {version}? The app closes while the installer runs.", "Open the download page for AI Usage Hub {version}? Replace the app in Applications with the new one."), { version: update.latestVersion }))) return;
    try {
      setInstalling(true);
      await api.installUpdate();
    } catch (error) {
      alert(String(error));
    } finally {
      setInstalling(false);
    }
  }

  return (
    <button
      type="button"
      className="primary-button update-button"
      onClick={() => void install()}
      disabled={installing}
      title={t("Version {version} is available on GitHub", { version: update.latestVersion })}
    >
      <Download size={14} className={installing ? "spin" : ""} />
      {installing ? t(byPlatform("Downloading update...", "Opening download page...")) : t("Update to {version}", { version: update.latestVersion })}
    </button>
  );
}
