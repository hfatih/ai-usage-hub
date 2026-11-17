import { ExternalLink, Link2, LogIn, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "../../i18n";
import { api, type ClaudeProfileStatus } from "../../lib/ipc";
import { useHub } from "../../stores/hub";
import { CliProfilePool, useCliProfiles, type ProfileCard, type ProfileName } from "./CliProfilePool";

const signedOut: ClaudeProfileStatus = { loggedIn: false, email: null, subscriptionType: null };

export function ClaudeAccounts() {
  const { t } = useTranslation();
  const [profile, setProfile] = useState<string | null>(null);
  const pool = useCliProfiles("claude", api.claudeProfileStatus);
  const accounts = useHub((state) => state.accounts);
  const refresh = useHub((state) => state.refresh);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [login, setLogin] = useState<string | null>(null);
  const pending = useRef<string | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (pending.current) void api.cancelClaudeLogin(pending.current).catch(() => {});
    };
  }, []);

  function clearLogin() {
    pending.current = null;
    setLogin(null);
  }

  useEffect(() => {
    if (!login) return;
    const id = login;
    let active = true;
    let next: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const result = await api.pollClaudeLogin(id);
        if (!active || pending.current !== id) return;
        if (!result) {
          next = setTimeout(() => void poll(), 1100);
          return;
        }
        pool.put(profile, result);
        const duplicate = result.email && [...pool.readings].some(([name, reading]) =>
          name !== profile && reading.data?.loggedIn && reading.data.email?.toLowerCase() === result.email?.toLowerCase());
        setNotice(duplicate ? "This email is already saved. To add another account, choose a different email in the browser."
          : profile === null ? "CLI sign-in saved." : "Sign-in saved. Select Switch on the account card to use it in your normal CLI.");
        clearLogin();
        void refresh();
      } catch (err) {
        if (active && pending.current === id) {
          void api.cancelClaudeLogin(id).catch(() => {});
          setError(String(err));
          setNotice("");
          clearLogin();
          void pool.reload();
        }
      }
    }
    next = setTimeout(() => void poll(), 1100);
    return () => { active = false; clearTimeout(next); };
  }, [login]);

  async function copy(id: string) {
    setError("");
    try {
      const url = await api.getClaudeLoginUrl(id);
      try { await navigator.clipboard.writeText(url); }
      catch { await api.copyClaudeLoginUrl(id); }
      setNotice("Claude sign-in link copied. Open it in your preferred browser on this PC.");
    } catch {
      setError("Could not copy the sign-in link. Try again or open the sign-in page.");
    }
  }

  function selectProfile(name: ProfileName) {
    setProfile(name);
    setNotice("");
    setError("");
  }

  async function createProfile(name: string) {
    const added = await pool.create(name);
    if (mounted.current) {
      pool.put(added, signedOut);
      selectProfile(added);
    }
    return added;
  }

  async function signIn(openBrowser: boolean, name = profile) {
    setBusy(true);
    setError("");
    setNotice("");
    let target = name;
    try {
      if (pool.readings.get(name)?.data?.loggedIn) {
        const used = new Set(pool.names.map((item) => item?.toLowerCase()));
        let number = 2;
        while (used.has(`claude ${number}`)) number += 1;
        const unfinished = pool.names.find((item) => item != null && pool.readings.get(item)?.data?.loggedIn === false);
        target = unfinished ?? await createProfile(`Claude ${number}`);
      }
      if (!mounted.current) return;
      setProfile(target);
      const result = await api.startClaudeProfileLogin(target, openBrowser);
      if (!mounted.current) {
        void api.cancelClaudeLogin(result.loginId).catch(() => {});
        return;
      }
      pending.current = result.loginId;
      setLogin(result.loginId);
      if (!openBrowser) await copy(result.loginId);
      else if (!result.browserOpened) setNotice("The browser could not be opened. Select Open sign-in page to try again.");
    } catch (err) {
      if (mounted.current) setError(String(err));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }

  async function cancel(id: string) {
    pending.current = null;
    setBusy(true);
    try { await api.cancelClaudeLogin(id); }
    catch (err) { setError(String(err)); }
    finally {
      clearLogin();
      setNotice("");
      setBusy(false);
    }
  }

  const locked = busy || login != null || pool.loading;
  const ready = pool.readings.get(profile)?.data != null;
  const cliAccount = pool.readings.get(null)?.data;
  const cards: ProfileCard[] = pool.names.map((name) => {
    const reading = pool.readings.get(name);
    const status = reading?.data;
    const email = status?.loggedIn ? status.email : null;
    const snapshot = email && accounts.find((account) =>
      account.providerId === "anthropic" && account.email.toLowerCase() === email.toLowerCase());
    return {
      name, email, signedIn: status?.loggedIn ?? false,
      active: !!email && cliAccount?.loggedIn === true && cliAccount.email?.toLowerCase() === email.toLowerCase(),
      accountKey: email?.toLowerCase(),
      plan: status?.subscriptionType ?? (snapshot ? snapshot.plan : null),
      quota: snapshot ? snapshot.usage : [],
      loading: pool.loading && !reading, error: reading?.error,
    };
  });
  return (
    <section className="codex-switcher" aria-labelledby="claude-profiles-title">
      <div className="page-heading">
        <div>
          <h2 id="claude-profiles-title">{t("Claude Code Account Pool")}</h2>
          <p>{t("Sign in once per account. Switch cards to change the account used by your normal CLI.")}</p>
        </div>
        <div className="hero-actions">
          <button type="button" className="secondary-button" disabled={locked || !ready} onClick={() => void signIn(false)}>
            <Link2 size={14} />{t("Copy sign-in link")}
          </button>
          <button type="button" className="primary-button" disabled={locked || !ready} onClick={() => void signIn(true)}>
            <LogIn size={14} />{t("Sign in with Google")}
          </button>
        </div>
      </div>
      {login && (
        <div className="glass-panel codex-login">
          <p role="status">{t("Choose Continue with Google on Claude’s sign-in page. Hub verifies the result automatically.")}</p>
          <div className="hero-actions">
            <button type="button" className="secondary-button" disabled={busy} onClick={() => void copy(login)}><Link2 size={14} />{t("Copy sign-in link")}</button>
            <button type="button" className="secondary-button" disabled={busy} onClick={() => {
              void api.openClaudeLogin(login).catch((err) => setError(String(err)));
            }}><ExternalLink size={14} />{t("Open sign-in page")}</button>
            <button type="button" className="secondary-button" disabled={busy} onClick={() => void cancel(login)}><X size={14} />{t("Cancel")}</button>
          </div>
        </div>
      )}
      {notice && <p className="codex-banner" role="status">{t(notice)}</p>}
      {(error || pool.error) && <p className="codex-banner codex-banner--error" role="alert">{t((error || pool.error).replace(/^operation failed: /, ""))}</p>}
      <CliProfilePool provider="claude" cards={cards} selected={profile} disabled={locked} loading={pool.loading}
        onSelect={selectProfile} onSignIn={(name) => void signIn(true, name)}
        onCreate={async (name) => { await createProfile(name); }}
        onRefresh={async () => { setError(""); await Promise.all([pool.reload(), refresh()]); }} />
    </section>
  );
}
