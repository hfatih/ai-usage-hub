import { Globe, Power } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "../../i18n";
import { api } from "../../lib/ipc";
import { useHub } from "../../stores/hub";
import type { AntigravityAccount } from "../../types";

/** Short plan name for a Google tier id such as `g1-ultra-tier`; null when the tier is unknown. */
export function tierLabel(tier: string | null | undefined): string | null {
  if (!tier) return null;
  const lower = tier.toLowerCase();
  if (lower.includes("ultra")) return "Ultra";
  if (lower.includes("pro")) return "Pro";
  if (lower.includes("free")) return "Free";
  return tier;
}

type PoolSort = "quota" | "recent" | "email" | "status" | "gemini" | "claude";

/** Orders pool accounts for display; `gemini` and `claude` sort by that model group's 5-hour window, unknown values last. */
export function sortPoolAccounts(accounts: AntigravityAccount[], sortBy: PoolSort): AntigravityAccount[] {
  const group = (account: AntigravityAccount, key: "gemini5h" | "claude5h") => account.quotaDetails?.[key] ?? -1;
  return [...accounts].sort((a, b) => {
    if (sortBy === "quota") return (b.quotaRemainingPercent ?? 0) - (a.quotaRemainingPercent ?? 0);
    if (sortBy === "recent") return b.lastUsed - a.lastUsed;
    if (sortBy === "email") return a.email.localeCompare(b.email);
    if (sortBy === "status") return a.status.localeCompare(b.status);
    if (sortBy === "gemini") return group(b, "gemini5h") - group(a, "gemini5h");
    return group(b, "claude5h") - group(a, "claude5h");
  });
}

/** Sets, changes or clears the proxy one pool account's Google traffic goes through. */
export function AccountProxyButton({ account, compact = false }: { account: AntigravityAccount; compact?: boolean }) {
  const { t } = useTranslation();
  const loadAntigravityAccounts = useHub((state) => state.loadAntigravityAccounts);

  async function edit() {
    const value = window.prompt(
      t("Proxy for {email} (http://host:port or socks5://user:pass@host:port). Leave empty to connect directly.", { email: account.email }),
      account.proxyUrl ?? "",
    );
    if (value == null) return;
    try {
      await api.setAntigravityAccountProxy(account.id, value.trim());
      await loadAntigravityAccounts();
    } catch (reason) {
      alert(String(reason));
    }
  }

  return (
    <button
      type="button"
      className={compact ? "secondary-button-sm" : "secondary-button"}
      onClick={() => void edit()}
      title={account.proxyUrl ? t("This account uses its own proxy") : t("Set a proxy for this account")}
      style={account.proxyUrl ? { color: "#ffd3e0", borderColor: "rgba(240,171,197,.5)" } : undefined}
    >
      <Globe size={compact ? 11 : 13} />
      {t(account.proxyUrl ? "Proxy on" : "Proxy")}
    </button>
  );
}

/** Close and force-close buttons for the running Antigravity IDE. */
export function AntigravityProcessButtons({ onDone }: { onDone: () => void }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);

  async function close(force: boolean) {
    if (force && !window.confirm(t("Force-close Antigravity? Unsaved work in the IDE is lost."))) return;
    try {
      setBusy(true);
      await api.closeAntigravity(force);
      setTimeout(onDone, 1500);
    } catch (reason) {
      alert(String(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <button type="button" className="sync-session-btn" disabled={busy} onClick={() => void close(false)}>
        <Power size={13} />
        <span>{t("Close IDE")}</span>
      </button>
      <button type="button" className="sync-session-btn" disabled={busy} onClick={() => void close(true)}>
        <Power size={13} />
        <span>{t("Force close")}</span>
      </button>
    </>
  );
}
