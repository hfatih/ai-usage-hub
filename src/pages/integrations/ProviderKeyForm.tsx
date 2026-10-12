import { KeyRound } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "../../i18n";
import { api } from "../../lib/ipc";
import { useHub } from "../../stores/hub";

/** Providers connected by pasting an API key instead of signing in to a local app. */
export const KEYED_PROVIDERS = ["kimi", "grok", "deepseek"];

/** Saves or removes a provider's API key. The key is sent to the backend once and never read back. */
export function ProviderKeyForm({ providerId, connected }: { providerId: string; connected: boolean }) {
  const { t } = useTranslation();
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(value: string) {
    try {
      setBusy(true);
      setError(null);
      useHub.setState({ snapshot: await api.setProviderKey(providerId, value) });
      useHub.setState({ accounts: await api.accounts() });
      setKey("");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form
      className="proxy-key-row"
      style={{ margin: "0 18px 12px" }}
      onSubmit={(event) => {
        event.preventDefault();
        if (key.trim()) void submit(key.trim());
      }}
    >
      <KeyRound size={14} />
      <input className="search-input" type="password" autoComplete="off" value={key} placeholder={t(connected ? "Paste a new API key" : "Paste your API key")} onChange={(event) => setKey(event.target.value)} style={{ flex: 1, minWidth: 0 }} />
      <button type="submit" className="secondary-button" disabled={busy || !key.trim()}>{t("Save")}</button>
      {connected && <button type="button" className="danger-button" disabled={busy} onClick={() => void submit("")}>{t("Remove key")}</button>}
      {error && <small style={{ color: "#f87171", flexBasis: "100%" }}>{error}</small>}
    </form>
  );
}
