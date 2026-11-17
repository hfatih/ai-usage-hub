import { ArrowRightLeft, ExternalLink, Link2, LogIn, ShieldCheck, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation, localizeValue } from "../../i18n";
import { codexSnapshot, codexWorkspace } from "../../lib/codexAccounts";
import { api } from "../../lib/ipc";
import { useHub } from "../../stores/hub";
import type { CodexSession } from "../../types";

import { CliProfilePool, useCliProfiles, type ProfileCard, type ProfileName } from "./CliProfilePool";

export function CodexAccounts() {
  const { t, language } = useTranslation();
  const accounts = useHub((state) => state.accounts);
  const refresh = useHub((state) => state.refresh);
  const [profile, setProfile] = useState<string | null>(null);
  const pool = useCliProfiles("codex", api.codexAccounts);
  const [selected, setSelected] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [login, setLogin] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const mounted = useRef(true);
  const pending = useRef<string | null>(null);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (pending.current) void api.cancelCodexLogin(pending.current).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (!login) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const result = await api.pollCodexLogin(login!);
        if (disposed || pending.current !== login) return;
        if (result) {
          pending.current = null;
          setLogin(null);
          pool.put(profile, result);
          const email = result.find((session) => session.isActive)?.email?.toLowerCase();
          const duplicate = email && [...pool.readings].some(([name, reading]) =>
            name !== profile && reading.data?.some((session) => session.isActive && session.email?.toLowerCase() === email));
          setNotice(duplicate ? "This email is already saved. To add another account, choose a different email in the browser."
            : profile === null ? "CLI sign-in saved." : "Sign-in saved. Select Switch on the account card to use it in your normal CLI.");
          void refresh();
        } else { timer = setTimeout(() => void poll(), 1200); }
      } catch (err) {
        if (disposed || pending.current !== login) return;
        // A transport failure can leave the owned callback pending; explicitly close it.
        void api.cancelCodexLogin(login!).catch(() => {});
        pending.current = null;
        setLogin(null);
        setError(String(err));
        void reloadAfterFailure();
      }
    }
    timer = setTimeout(() => void poll(), 1200);
    return () => { disposed = true; clearTimeout(timer); };
  }, [login]);

  async function reloadAfterFailure() {
    try { const result = await api.codexAccounts(profile); if (mounted.current) pool.put(profile, result); } catch { /* Keep the actionable original error. */ }
  }

  async function copyLoginLink(loginId: string) {
    setBusy("copy"); setError(null);
    try {
      const url = await api.getCodexLoginUrl(loginId);
      try {
        await navigator.clipboard.writeText(url);
      } catch {
        await api.copyCodexLoginUrl(loginId);
      }
      if (mounted.current) setNotice("Local sign-in link copied. Open it in your preferred browser on this PC.");
    } catch {
      if (mounted.current) setError(t("Could not copy the sign-in link. Try again or open the sign-in page."));
    } finally { if (mounted.current) setBusy(null); }
  }

  async function createProfile(name: string) {
    const added = await pool.create(name);
    if (mounted.current) {
      pool.put(added, []);
      selectProfile(added);
    }
    return added;
  }

  async function startLogin(openBrowser = true, name = profile) {
    setBusy("login"); setError(null); setNotice(null);
    try {
      let target = name;
      if (pool.readings.get(name)?.data?.some((session) => session.isActive)) {
        const existing = new Set(pool.names.map((item) => item?.toLowerCase()));
        let number = 2;
        while (existing.has("chatgpt " + number)) number += 1;
        const unfinished = pool.names.find((item) => item != null && pool.readings.get(item)?.data?.length === 0);
        target = unfinished ?? await createProfile("ChatGPT " + number);
        if (!mounted.current) return;
      }
      setProfile(target);
      const result = await api.startCodexLogin(openBrowser, target);
      if (!mounted.current) { void api.cancelCodexLogin(result.loginId).catch(() => {}); return; }
      pending.current = result.loginId;
      setLogin(result.loginId);
      if (!openBrowser) await copyLoginLink(result.loginId);
      if (openBrowser && !result.browserOpened) setNotice("The browser could not be opened. Select Open sign-in page to try again.");
    } catch (err) { if (mounted.current) setError(String(err)); }
    finally { if (mounted.current) setBusy(null); }
  }

  async function cancelLogin() {
    if (!login) return;
    const id = login;
    pending.current = null; setLogin(null); setBusy("cancel"); setNotice(null);
    try { await api.cancelCodexLogin(id); }
    catch (err) { if (mounted.current) setError(String(err)); }
    finally { if (mounted.current) { setBusy(null); void reloadAfterFailure(); } }
  }

  async function switchAccount(name: ProfileName, session: CodexSession, workspace: string) {
    setBusy(session.sessionId); setError(null); setNotice(null);
    try {
      const result = await api.switchCodexAccount(session.sessionId, workspace, name);
      if (mounted.current) {
        pool.put(name, result);
        if (name && session.email) await api.switchAccountProfile("codex", name, session.email);
        await Promise.all([pool.reload(), refresh()]);
        if (mounted.current) setNotice("CLI account switched. Open a new CLI session to use this account.");
      }
    } catch (err) { if (mounted.current) { setError(String(err)); } }
    finally { if (mounted.current) setBusy(null); }
  }

  const disabled = busy != null || login != null || pool.loading;
  const ready = pool.readings.get(profile)?.data != null;
  const cliAccount = pool.readings.get(null)?.data?.find((session) => session.isActive);

  function selectProfile(name: ProfileName) {
    setProfile(name);
    setNotice(null);
    setError(null);
  }

  const cards: ProfileCard[] = pool.names.flatMap((name) => {
    const reading = pool.readings.get(name);
    const sessions = reading?.data ?? [];
    return (sessions.length ? sessions : [null]).map((session) => {
      const key = JSON.stringify([name, session?.sessionId]);
      const workspace = session ? codexWorkspace(session, selected[key]) : "";
      const snapshot = session && codexSnapshot(session, workspace, accounts);
      return {
        name, id: key, inactive: !!session && !session.isActive,
        email: session?.email, plan: snapshot ? snapshot.plan : null,
        signedIn: session?.isActive ?? false,
        active: !!session && session.isActive && session.email?.toLowerCase() === cliAccount?.email?.toLowerCase()
          && session.selectedWorkspaceAccountId === cliAccount?.selectedWorkspaceAccountId,
        accountKey: session?.email ? JSON.stringify([session.email.toLowerCase(), session.selectedWorkspaceAccountId]) : undefined,
        lastUsed: session?.lastUsedAt, quota: snapshot ? snapshot.usage : [],
        loading: pool.loading && !reading, error: reading?.error,
        details: session && (
          <div className="cli-card-workspaces">
            <label className="codex-workspace">
              <span>{t("Workspace")}</span>
              <select aria-label={t("Workspace for {email}", { email: session.email ?? t("ChatGPT account") })} value={workspace}
                disabled={disabled || session.workspaces.length < 2}
                onChange={(event) => setSelected((previous) => ({ ...previous, [key]: event.target.value }))}>
                {session.workspaces.length === 0 && <option value="">{t("Sign in again to load workspaces")}</option>}
                {session.workspaces.map((item) => <option key={item.accountId} value={item.accountId}>{item.name ?? item.accountId}</option>)}
              </select>
            </label>
            {(!session.isActive || workspace !== session.selectedWorkspaceAccountId) && workspace && (
              <button type="button" className="secondary-button" disabled={disabled} onClick={() => void switchAccount(name, session, workspace)}>
                <ArrowRightLeft size={14} />{t("Switch workspace")}
              </button>
            )}
          </div>
        ),
      };
    });
  });

  return (
    <section className="codex-switcher" aria-labelledby="codex-switcher-title">
      <div className="page-heading">
        <div>
          <span className="eyebrow">ChatGPT / Codex</span>
          <h1 id="codex-switcher-title">{t("Codex Account Pool")}</h1>
          <p>{t("Sign in once per account, then switch using Codex's saved local sessions.")}</p>
        </div>
        <div className="hero-actions">
          <button type="button" className="secondary-button" onClick={() => void startLogin(false)} disabled={disabled || !ready} title={t("Copy sign-in link")} aria-label={t("Copy sign-in link")}>
            <Link2 size={14} />{t("Copy sign-in link")}
          </button>
          <button type="button" className="primary-button" onClick={() => void startLogin()} disabled={disabled || !ready}>
            <LogIn size={14} />{t("Sign in with ChatGPT")}
          </button>
        </div>
      </div>
      <p className="page-note">{t("Starting sign-in from a signed-in profile creates a new account profile.")}</p>
      <p className="page-note codex-privacy"><ShieldCheck size={14} />{t("Saved sign-ins stay on this PC. Switching updates the CLI’s local sign-in.")}</p>
      {(error || pool.error) && <div className="codex-banner codex-banner--error" role="alert">{localizeValue((error || pool.error).replace(/^operation failed: /, ""), language)}</div>}
      {notice && <div className="codex-banner" role="status">{t(notice)}</div>}
      {login && (
        <div className="glass-panel codex-login" role="status">
          <span>{t("Complete sign-in in your browser. Your previous account stays saved.")}</span>
          <div className="hero-actions">
            <button type="button" className="secondary-button" disabled={busy != null} onClick={() => void copyLoginLink(login)}><Link2 size={14} />{t("Copy sign-in link")}</button>
            <button type="button" className="secondary-button" disabled={busy != null} onClick={() => void api.openCodexLogin(login).catch((err) => setError(String(err)))}><ExternalLink size={14} />{t("Open sign-in page")}</button>
            <button type="button" className="secondary-button" disabled={busy != null} onClick={() => void cancelLogin()}><X size={14} />{t("Cancel")}</button>
          </div>
        </div>
      )}
      <CliProfilePool provider="codex" cards={cards} selected={profile} disabled={disabled} loading={pool.loading}
        onSelect={selectProfile} onSignIn={(name) => void startLogin(true, name)}
        onCreate={async (name) => { await createProfile(name); }}
        onRefresh={async () => { setError(null); await Promise.all([pool.reload(), refresh()]); }} />
    </section>
  );
}
