import { ArrowRightLeft, Database, FileJson, FileKey2, HardDrive, KeyRound, Server, ShieldCheck, Trash2 } from "lucide-react";
import { localizeValue, useTranslation } from "../i18n";
import { useHub } from "../stores/hub";

export function SecurityPage() {
  const { language, t } = useTranslation();
  const { security, settings, antigravityAccounts, clearUsageHistory } = useHub();
  if (!security) return null;
  const storesTokens = security.storedSecretCount > 0;
  async function clear() {
    if (!window.confirm(t("Delete all local usage samples and usage alerts? Account records will be kept."))) return;
    await clearUsageHistory();
  }
  const notes = [
    { icon: ShieldCheck, title: "Authentication boundary", text: "Codex and Claude Code keep their own credentials; only labels and usage snapshots are stored for them. Antigravity pool accounts are different: their Google sign-in tokens are saved here, encrypted with Windows data protection, so the hub can switch accounts and run the proxy. A copied database cannot be read under another Windows user." },
    { icon: ArrowRightLeft, title: "Account switching", text: "Switching writes the chosen account's token to Windows Credential Manager (gemini:antigravity) and restarts Antigravity's language server." },
    { icon: Server, title: "Local API proxy", text: settings?.proxyRequireKey === false ? "Listens on 127.0.0.1 only. The API key check is turned off: any program on this PC can use the pooled accounts through it while the hub is running." : "Listens on 127.0.0.1 only and rejects requests without the API key shown on the API Proxy page." },
    { icon: FileJson, title: "Account export", text: "An exported JSON file contains the same sign-in tokens in plain text. Treat it like a password." },
  ];
  return <section><div className="page-heading"><div><span className="eyebrow">{t("Local privacy")}</span><h1>{t("Security")}</h1><p>{t("See what is stored, where it lives and how to clear it.")}</p></div></div>
    <div className="security-grid"><article className="glass-panel security-card">{storesTokens ? <KeyRound size={22} /> : <ShieldCheck size={22} />}<strong>{t(storesTokens ? "Sign-in tokens stored locally" : "No provider secrets stored")}</strong><span>{storesTokens ? t(security.secretsEncrypted ? "{count} Google sign-in tokens for {accounts} Antigravity pool accounts, encrypted for your Windows account." : "{count} Google sign-in tokens for {accounts} Antigravity pool accounts, unencrypted in this database.", { count: security.storedSecretCount, accounts: antigravityAccounts.length }) : t("{count} passwords, tokens or API keys in this database.", { count: 0 })}</span></article><article className="glass-panel security-card"><Database size={22} /><strong>{t(security.accountCount === 1 ? "{count} saved account security" : "{count} saved accounts security", { count: security.accountCount })}</strong><span>{t("{count} verified usage samples", { count: security.usageSnapshotCount })}</span></article><article className="glass-panel security-card"><HardDrive size={22} /><strong>{t("{count} KB local data", { count: Math.max(1, Math.round(security.databaseSizeBytes / 1024)) })}</strong><span>{t("Stored only in the current Windows profile.")}</span></article></div>
    <div className="glass-panel security-details"><div><FileKey2 size={19} /><span><strong>{t("Database location")}</strong><small>{localizeValue(security.databasePath, language)}</small></span></div>{notes.map(({ icon: Icon, title, text }) => <div key={title}><Icon size={19} /><span><strong>{t(title)}</strong><small>{t(text)}</small></span></div>)}<div><Trash2 size={19} /><span><strong>{t("Usage history")}</strong><small>{t("Remove snapshots and generated usage alerts while keeping account names.")}</small></span><button className="danger-button" onClick={() => void clear()}><Trash2 size={14} />{t("Clear usage history")}</button></div></div>
  </section>;
}
