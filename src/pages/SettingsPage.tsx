import { LaptopMinimal, Moon, Power, Save, Sliders, SunMoon, Zap } from "lucide-react";
import { useEffect, useState } from "react";
import { useHub } from "../stores/hub";
import { useTranslation } from "../i18n";
import type { AppSettings } from "../types";

export function SettingsPage() {
  const { t } = useTranslation();
  const { settings, saveSettings } = useHub();
  const [draft, setDraft] = useState<AppSettings | null>(settings);
  useEffect(() => setDraft(settings), [settings]);
  if (!draft) return null;
  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => setDraft({ ...draft, [key]: value });
  const changeLanguage = (language: string) => {
    const next = { ...draft, language };
    setDraft(next);
    void saveSettings(next);
  };

  return (
    <section>
      <div className="page-heading">
        <div>
          <span className="eyebrow">{t("Preferences")}</span>
          <h1>{t("Settings")}</h1>
          <p>{t("Desktop behavior and local monitoring controls.")}</p>
        </div>
        <button className="primary-button" onClick={() => void saveSettings(draft)}>
          <Save size={16} />
          {t("Save changes")}
        </button>
      </div>
      <div className="settings-layout">
        <aside className="settings-nav glass-panel">
          <button className="is-active">
            <LaptopMinimal size={17} />
            {t("General")}
          </button>
        </aside>
        <div className="settings-panel glass-panel">
          <div className="settings-section">
            <div>
              <Power size={19} />
              <span>
                <strong>{t("Windows startup")}</strong>
                <small>{t("Start quietly in the system tray after sign-in.")}</small>
              </span>
            </div>
            <label className="switch">
              <input type="checkbox" checked={draft.launchAtStartup} onChange={(event) => update("launchAtStartup", event.target.checked)} />
              <span />
            </label>
          </div>
          <div className="settings-section">
            <div>
              <LaptopMinimal size={19} />
              <span>
                <strong>{t("Process monitoring")}</strong>
                <small>{t("Observe configured process names in the background.")}</small>
              </span>
            </div>
            <label className="switch">
              <input type="checkbox" checked={draft.processMonitoring} onChange={(event) => update("processMonitoring", event.target.checked)} />
              <span />
            </label>
          </div>
          <div className="settings-section">
            <div>
              <SunMoon size={19} />
              <span>
                <strong>{t("Theme")}</strong>
                <small>{t("Choose the appearance for this device.")}</small>
              </span>
            </div>
            <select value={draft.theme} onChange={(event) => update("theme", event.target.value as AppSettings["theme"])}>
              <option value="dark">{t("Dark")}</option>
              <option value="light">{t("Light")}</option>
              <option value="system">{t("System")}</option>
            </select>
          </div>
          <div className="settings-section">
            <div>
              <Moon size={19} />
              <span>
                <strong>{t("Close button")}</strong>
                <small>{t("Keep monitoring without leaving a window open.")}</small>
              </span>
            </div>
            <select value={draft.closeBehavior} onChange={(event) => update("closeBehavior", event.target.value as AppSettings["closeBehavior"])}>
              <option value="tray">{t("Minimize to tray")}</option>
              <option value="quit">{t("Quit application")}</option>
            </select>
          </div>
          <div className="settings-section">
            <div>
              <LaptopMinimal size={19} />
              <span>
                <strong>{t("Language")}</strong>
                <small>{t("Choose the interface language.")}</small>
              </span>
            </div>
            <select aria-label={t("Language")} value={draft.language === "en" ? "en" : "tr"} onChange={(event) => changeLanguage(event.target.value)}>
              <option value="tr">{t("Turkish")}</option>
              <option value="en">{t("English")}</option>
            </select>
          </div>

          <div className="settings-group-title">{t("Antigravity Auto-Switcher")}</div>
          <div className="settings-section">
            <div>
              <Zap size={19} />
              <span>
                <strong>{t("Auto-Switcher")}</strong>
                <small>{t("Automatically rotate to the next healthy account when quota drops below threshold.")}</small>
              </span>
            </div>
            <label className="switch">
              <input type="checkbox" checked={draft.antigravityAutoSwitch} onChange={(e) => update("antigravityAutoSwitch", e.target.checked)} />
              <span />
            </label>
          </div>
          <div className="settings-section">
            <div>
              <Sliders size={19} />
              <span>
                <strong>{t("Switch threshold")}</strong>
                <small>{t("Create a warning when remaining usage crosses this value.")} ({draft.antigravitySwitchThreshold}%)</small>
              </span>
            </div>
            <div className="range-control">
              <input
                type="range"
                min={1}
                max={50}
                value={draft.antigravitySwitchThreshold}
                onChange={(e) => update("antigravitySwitchThreshold", Number(e.target.value))}
              />
              <span>{draft.antigravitySwitchThreshold}%</span>
            </div>
          </div>
          <div className="settings-section">
            <div>
              <Zap size={19} />
              <span>
                <strong>{t("Switch on rate limit")}</strong>
                <small>{t("Rotate on rate limit")}</small>
              </span>
            </div>
            <label className="switch">
              <input type="checkbox" checked={draft.antigravitySwitchOnRateLimit} onChange={(e) => update("antigravitySwitchOnRateLimit", e.target.checked)} />
              <span />
            </label>
          </div>
          <div className="settings-section">
            <div>
              <LaptopMinimal size={19} />
              <span>
                <strong>{t("Switch strategy")}</strong>
                <small>{t("Strategy")}</small>
              </span>
            </div>
            <select value={draft.antigravitySwitchStrategy} onChange={(e) => update("antigravitySwitchStrategy", e.target.value)}>
              <option value="highest_quota">{t("Highest available quota")}</option>
              <option value="round_robin">{t("Round-robin (least recently used)")}</option>
            </select>
          </div>
        </div>
      </div>
    </section>
  );
}
