import { useState } from "react";
import { ArrowLeft, ArrowRight, BellRing, Check, Eye, Radar, ShieldCheck, Sparkles } from "lucide-react";
import { localizeValue, useTranslation } from "../i18n";
import { useHub } from "../stores/hub";
import { BrandMark } from "./BrandMark";
import { byPlatform } from "../lib/platform";

interface Props { appName: string; tagline: string; onComplete: () => void }

export function SetupWizard({ appName, tagline, onComplete }: Props) {
  const [step, setStep] = useState(0);
  const { snapshot, refresh } = useHub();
  const { language, t } = useTranslation();
  const steps = [
    { title: "Welcome", icon: Sparkles },
    { title: "Detect apps", icon: Radar },
    { title: "Providers", icon: ShieldCheck },
    { title: "Notifications", icon: BellRing },
    { title: "Ready", icon: Check },
  ];
  const current = steps[step];
  const CurrentIcon = current.icon;

  return (
    <div className="wizard-shell">
      <div className="wizard-window glass-panel">
        <div className="wizard-brand"><BrandMark /><span>{appName}</span></div>
        <div className="wizard-progress" aria-label={t("Setup step {step} of {total}", { step: step + 1, total: steps.length })}>
          {steps.map((item, index) => <span key={item.title} className={index <= step ? "is-complete" : ""} />)}
        </div>
        <div className="wizard-body">
          {step === 0 && <><span className="wizard-icon"><CurrentIcon size={27} /></span><h1>{t("Your AI tools.")}<br /><em>{t("One calm dashboard.")}</em></h1><p>{t(tagline)}</p><div className="privacy-note"><Eye size={17} /><span>{t("Local-first. No telemetry. No passwords collected.")}</span></div></>}
          {step === 1 && <><span className="wizard-icon"><CurrentIcon size={27} /></span><h2>{t("Detect your apps")}</h2><p>{t("We look only at running process names and known install locations.")}</p><div className="detected-grid">{snapshot?.applications.slice(0, 8).map((app) => <div key={app.id} className="detected-app"><span className={`app-orb app-orb--${app.providerId ?? "local"}`}>{app.displayName.slice(0, 1)}</span><span>{app.displayName}</span><small>{t(app.state === "running" ? "Running" : app.launchable ? "Found" : "Not detected")}</small></div>)}</div><button className="secondary-button" onClick={() => void refresh()}><Radar size={16} />{t("Scan again")}</button></>}
          {step === 2 && <><span className="wizard-icon"><CurrentIcon size={27} /></span><h2>{t("Provider status")}</h2><p>{t("ChatGPT and Codex share the plan allowance. The hub reads it from your current Codex sign-in without asking for a password.")}</p><div className="provider-list">{snapshot?.providers.slice(0, 4).map((provider) => <div key={provider.id}><span>{provider.name}</span><small>{provider.connectionStatus === "connected" ? localizeValue(provider.accountLabel, language) : t("Not connected")}</small><button disabled>{t(provider.connectionStatus === "connected" ? "Connected" : "Unavailable")}</button></div>)}</div></>}
          {step === 3 && <><span className="wizard-icon"><CurrentIcon size={27} /></span><h2>{t("Useful, not noisy")}</h2><p>{t("Notifications only fire from reliable provider limits. These defaults can be changed later.")}</p><div className="notification-options">{[20, 10, 5].map((count, index) => <label key={count}><span>{t("{count}% remaining", { count })}</span><input type="checkbox" defaultChecked={index === 0} /></label>)}<label><span>{t("Limit reached")}</span><input type="checkbox" defaultChecked /></label></div></>}
          {step === 4 && <><span className="wizard-icon wizard-icon--success"><CurrentIcon size={27} /></span><h2>{t("Everything is ready")}</h2><p>{t("Continue using your favorite AI apps normally. The hub will observe app-open time in the background.")}</p><div className="ready-card"><ShieldCheck size={20} /><div><strong>{t("Private by default")}</strong><span>{t(byPlatform("Data stays on this PC in a local SQLite database.", "Data stays on this Mac in a local SQLite database."))}</span></div></div></>}
        </div>
        <footer className="wizard-actions">
          <button className="text-button" disabled={step === 0} onClick={() => setStep((value) => value - 1)}><ArrowLeft size={16} />{t("Back")}</button>
          <button className="primary-button" onClick={() => step === steps.length - 1 ? onComplete() : setStep((value) => value + 1)}>{t(step === steps.length - 1 ? "Open dashboard" : step === 0 ? "Get started" : "Continue")}<ArrowRight size={16} /></button>
        </footer>
      </div>
    </div>
  );
}
