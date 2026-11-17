import { ArrowRightLeft, ArrowUpDown, Check, FolderOpen, LayoutGrid, List, LogIn, Plus, RefreshCw, Search, Trash2, Undo2 } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { localizeValue, useTranslation } from "../../i18n";
import { formatUsageValue, relativeTime, usageProgress, usageTiming } from "../../lib/format";
import { api, type AccountProfileProvider } from "../../lib/ipc";
import type { UsageValue } from "../../types";

export type ProfileName = string | null;
type ReadProfile<T> = (profile: ProfileName) => Promise<T>;
type Readings<T> = Map<ProfileName, { data?: T; error?: string }>;

export function useCliProfiles<T>(provider: AccountProfileProvider, read: ReadProfile<T>) {
  const [names, setNames] = useState<ProfileName[]>([null]);
  const [readings, setReadings] = useState<Readings<T>>(new Map());
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const generation = useRef(0);

  async function reload() {
    const version = ++generation.current;
    setLoading(true);
    setError("");
    try {
      const profiles: ProfileName[] = [null, ...await api.accountProfiles(provider)];
      if (generation.current !== version) return;
      setNames(profiles);
      for (const name of profiles) {
        let reading: { data?: T; error?: string };
        try { reading = { data: await read(name) }; }
        catch (err) { reading = { error: String(err) }; }
        if (generation.current !== version) return;
        setReadings((previous) => new Map(previous).set(name, reading));
      }
    } catch (err) {
      if (generation.current === version) setError(String(err));
    } finally {
      if (generation.current === version) setLoading(false);
    }
  }

  useEffect(() => {
    void reload();
    return () => { generation.current += 1; };
  }, [provider, read]);

  function put(name: ProfileName, data: T) {
    setReadings((previous) => new Map(previous).set(name, { data }));
  }

  async function create(name: string) {
    const added = await api.createAccountProfile(provider, name);
    setNames((previous) => [...new Set([...previous, added])]);
    return added;
  }

  return { names, readings, loading, error, reload, put, create };
}

export interface ProfileCard {
  name: ProfileName;
  id?: string;
  inactive?: boolean;
  active?: boolean;
  accountKey?: string;
  email?: string | null;
  plan?: string | null;
  signedIn: boolean;
  lastUsed?: number | null;
  quota: UsageValue[];
  loading?: boolean;
  error?: string;
  details?: ReactNode;
}

function legacyRemovalKey(card: ProfileCard) {
  return JSON.stringify([card.name, card.id ?? card.email?.toLowerCase() ?? null]);
}

function removalKey(card: ProfileCard) {
  return card.accountKey ? JSON.stringify(["account", card.accountKey]) : legacyRemovalKey(card);
}

function readRemovedAccounts(key: string): Set<string> {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(key) ?? "[]");
    if (Array.isArray(saved)) return new Set(saved.filter((value): value is string => typeof value === "string"));
  } catch {
    // A missing or unreadable preference leaves all cards visible.
  }
  return new Set();
}

type Sort = "profile" | "email" | "quota" | "recent";
const quotaValue = (card: ProfileCard) => {
  const remaining = card.quota.filter((usage) => usage.unit === "% remaining")
    .map(usageProgress).filter((value): value is number => value != null);
  return remaining.length > 0 ? Math.min(...remaining) : null;
};

export function CliProfilePool({
  provider, cards, selected, disabled, loading, onSelect, onSignIn, onCreate, onRefresh,
}: {
  provider: AccountProfileProvider;
  cards: ProfileCard[];
  selected: ProfileName;
  disabled: boolean;
  loading: boolean;
  onSelect: (name: ProfileName) => void;
  onSignIn: (name: ProfileName) => void;
  onCreate: (name: string) => Promise<void>;
  onRefresh: () => Promise<void>;
}) {
  const { t, language } = useTranslation();
  const storageKey = "ai-usage-hub.removed-accounts." + provider;
  const [savedRemoved, setRemoved] = useState(() => readRemovedAccounts(storageKey));
  const [showRemoved, setShowRemoved] = useState(false);
  const [view, setView] = useState<"grid" | "list">("grid");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>("profile");
  const [draft, setDraft] = useState("");
  const [adding, setAdding] = useState(false);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [switching, setSwitching] = useState<string | null>(null);
  const label = (name: ProfileName) => name ?? t("Default profile");
  const locked = disabled || working;
  const needle = query.trim().toLocaleLowerCase(language);
  const isSelected = (card: ProfileCard) => card.name === selected && !card.inactive;
  const unique = cards.filter((card, index) => !card.accountKey || cards.findIndex((item) => item.accountKey === card.accountKey) === index);
  const removed = new Set(savedRemoved);
  for (const card of cards) {
    const previousKey = legacyRemovalKey(card);
    const key = removalKey(card);
    if (key !== previousKey && removed.delete(previousKey)) removed.add(key);
  }
  useEffect(() => {
    if (removed.size === savedRemoved.size && [...removed].every((key) => savedRemoved.has(key))) return;
    try {
      localStorage.setItem(storageKey, JSON.stringify([...removed]));
      setRemoved(removed);
    } catch {
      setError("Could not save the account list.");
    }
  }, [cards, savedRemoved, storageKey]);

  const matchesQuery = (card: ProfileCard) => [label(card.name), card.email, card.plan].some((part) =>
    part?.toLocaleLowerCase(language).includes(needle));
  const removedCards = unique.filter((card) => removed.has(removalKey(card)));
  const drafts = unique.filter((card) => !card.signedIn && !card.inactive && !removed.has(removalKey(card))).filter(matchesQuery);
  const viewingRemoved = showRemoved && removedCards.length > 0;
  const shown = (viewingRemoved ? removedCards : unique.filter((card) => (card.signedIn || card.inactive) && !removed.has(removalKey(card)))).filter(matchesQuery).sort((a, b) => {
    const byName = label(a.name).localeCompare(label(b.name), language, { numeric: true });
    if (sort === "quota") return (quotaValue(b) ?? -1) - (quotaValue(a) ?? -1) || byName;
    if (sort === "recent") return (b.lastUsed ?? 0) - (a.lastUsed ?? 0) || byName;
    if (sort === "email") return (a.email ?? label(a.name)).localeCompare(b.email ?? label(b.name), language);
    return Number(b.name === null) - Number(a.name === null) || byName;
  });

  useEffect(() => {
    if (locked || !cards.some((card) => card.name === selected && !card.inactive && removed.has(removalKey(card)))) return;
    const visible = cards.filter((card) => !card.inactive && !card.loading && !card.error && !removed.has(removalKey(card)));
    const fallback = visible.find((card) => card.signedIn) ?? visible[0];
    if (fallback) onSelect(fallback.name);
  }, [cards, locked, onSelect, removed, selected]);

  function toggleRemoved(card: ProfileCard, restore: boolean) {
    const key = removalKey(card);
    if (!restore && !window.confirm(t("Remove {account} from the Hub account pool? The CLI stays signed in.", {
      account: card.email || label(card.name),
    }))) return;
    const next = new Set(removed);
    if (restore) next.delete(key);
    else next.add(key);
    try {
      localStorage.setItem(storageKey, JSON.stringify([...next]));
    } catch {
      setError("Could not save the account list.");
      return;
    }
    setError("");
    setRemoved(next);
    if (restore && removedCards.length === 1) setShowRemoved(false);
  }

  async function activate(card: ProfileCard) {
    if (!card.name || !card.email) return;
    setWorking(true);
    setSwitching(removalKey(card));
    setError("");
    setNotice("");
    try {
      await api.switchAccountProfile(provider, card.name, card.email);
      await onRefresh();
      onSelect(card.name);
      setNotice("CLI account switched. Open a new CLI session to use this account.");
    } catch (err) {
      setError(String(err));
      await onRefresh().catch(() => {});
    } finally {
      setWorking(false);
      setSwitching(null);
    }
  }

  async function add(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!draft.trim()) return;
    setWorking(true);
    setError("");
    try {
      await onCreate(draft);
      setAdding(false);
      setDraft("");
    } catch (err) {
      setError(String(err));
    } finally {
      setWorking(false);
    }
  }

  async function open(name: ProfileName) {
    setWorking(true);
    setError("");
    try {
      const folder = await api.chooseProjectFolder();
      if (!folder) return;
      await api.launchAccountProfile(provider, name, folder);
      onSelect(name);
    } catch (err) {
      setError(String(err));
    } finally {
      setWorking(false);
    }
  }

  return (
    <>
      <div className="cli-pool-toolbar">
        <label className="cli-pool-search">
          <Search size={14} aria-hidden="true" />
          <input className="search-input" type="search" aria-label={t("Search accounts...")} placeholder={t("Search accounts...")}
            value={query} onChange={(event) => setQuery(event.target.value)} />
        </label>
        <div className="cli-pool-primary-actions">
          <button type="button" className="secondary-button" disabled={locked || loading} onClick={() => {
            setError(""); setNotice("");
            void onRefresh().catch((err) => setError(String(err)));
          }}>
            <RefreshCw size={14} className={loading ? "spin" : ""} />{t("Refresh accounts")}
          </button>
          <button type="button" className="primary-button" disabled={locked} aria-expanded={adding} onClick={() => setAdding(!adding)}>
            <Plus size={14} />{t("Add account profile")}
          </button>
        </div>
        <div className="cli-pool-filters">
          <div className="view-mode-group">
            <button type="button" className={`view-mode-btn ${view === "grid" ? "is-active" : ""}`} title={t("Grid view")}
              aria-label={t("Grid view")} aria-pressed={view === "grid"} onClick={() => setView("grid")}><LayoutGrid size={14} /></button>
            <button type="button" className={`view-mode-btn ${view === "list" ? "is-active" : ""}`} title={t("List view")}
              aria-label={t("List view")} aria-pressed={view === "list"} onClick={() => setView("list")}><List size={14} /></button>
          </div>
          <label className="cli-pool-sort">
            <ArrowUpDown size={13} aria-hidden="true" />{t("Sort by")}
            <select value={sort} onChange={(event) => setSort(event.target.value as Sort)}>
              <option value="profile">{t("Account profile")}</option>
              <option value="email">{t("Email (A-Z)")}</option>
              <option value="quota">{t("Highest quota")}</option>
              {provider === "codex" && <option value="recent">{t("Recently used")}</option>}
            </select>
          </label>
          {removedCards.length > 0 && (
            <button type="button" className="secondary-button" aria-pressed={viewingRemoved} onClick={() => setShowRemoved(!viewingRemoved)}>
              {t(viewingRemoved ? "Show account pool" : "Show removed accounts")}
            </button>
          )}

        </div>
      </div>
      {adding && (
        <form className="glass-panel cli-profiles cli-profile-actions" onSubmit={(event) => void add(event)}>
          <label className="codex-workspace">
            <span>{t("Profile name")}</span>
            <input value={draft} onChange={(event) => setDraft(event.target.value)} maxLength={40} required />
          </label>
          <button type="submit" className="primary-button" disabled={locked}>{t("Create account profile")}</button>
        </form>
      )}
      {error && <p role="alert" className="codex-banner codex-banner--error">{t(error.replace(/^operation failed: /, ""))}</p>}
      {notice && <p role="status" className="codex-banner">{t(notice)}</p>}
      {shown.length === 0 && !loading && <p className="glass-panel codex-empty">{t(needle ? "No accounts matched your search" : "No accounts in pool")}</p>}
      <div className={`account-grid pool-grid cli-profile-grid ${view === "list" ? "cli-profile-grid--list" : ""}`}>
        {shown.map((card) => {
          const chosen = !viewingRemoved && card.active === true;
          const remaining = quotaValue(card);
          return (
            <article className={`glass-panel account-card cli-profile-card ${chosen ? "is-current-pool-card" : ""}`} key={card.id ?? card.name ?? ""}>
              <header>
                <span className={`provider-logo provider-logo--${provider === "codex" ? "openai" : "anthropic"}`}>{provider === "codex" ? "C" : "A"}</span>
                <div>
                  <small>{provider === "codex" ? "ChatGPT / Codex" : "Anthropic / Claude Code"}</small>
                  <strong>{card.email || label(card.name)}</strong>
                  <span>{[card.email && label(card.name), card.plan].filter(Boolean).join(" · ")}</span>
                </div>
                {chosen ? (
                  <span className="connection-pill connection-pill--connected"><Check size={12} />{t("Active CLI account")}</span>
                ) : !viewingRemoved && !card.inactive && card.signedIn && card.email && card.name != null && (
                  <button type="button" className="secondary-button switch-btn" disabled={locked || !!card.error || card.loading} onClick={() => void activate(card)}>
                    <ArrowRightLeft size={13} className={switching === removalKey(card) ? "spin" : ""} />{t(switching === removalKey(card) ? "Switching..." : "Switch")}
                  </button>
                )}
              </header>
              <div className="account-usage-list">
                <div className="account-usage">
                  <div><span>{t("Quota remaining")}{remaining != null && <> · {t("Last known snapshot")}</>}</span><strong>{remaining == null ? "—" : `${Math.round(remaining)}%`}</strong></div>
                  {remaining != null && <div className="usage-track"><span style={{ width: `${remaining}%` }} /></div>}
                  <small>{t(card.loading ? "Loading profile…" : card.error ? "Could not read profile" : card.signedIn ? "Connected" : "Not connected")}</small>
                </div>
                {card.quota.length > 0 && (
                  <div className="cli-quota-chips">
                    {card.quota.map((usage) => (
                      <div key={usage.metric} className="model-quota-chip">
                        <div className="model-quota-chip-header">
                          <span className="model-quota-label">{localizeValue(usage.label, language)}</span>
                          <strong className="model-quota-val">{formatUsageValue(usage)}</strong>
                        </div>
                        <span className="model-quota-reset">{usageTiming(usage)}</span>
                      </div>
                    ))}
                  </div>
                )}
                {card.error && <small className="cli-profile-error">{localizeValue(card.error.replace(/^operation failed: /, ""), language)}</small>}
                {!viewingRemoved && card.details}
              </div>
              <footer>
                <span>{card.lastUsed ? `${t("Last used")} · ${relativeTime(card.lastUsed)}` : ""}</span>
                {viewingRemoved ? (
                  <button type="button" className="secondary-button" disabled={locked} onClick={() => toggleRemoved(card, true)}>
                    <Undo2 size={13} />{t("Restore account")}
                  </button>
                ) : (
                  <div className="cli-card-actions">
                    {card.signedIn ? (
                      <button type="button" className="secondary-button" disabled={locked} onClick={() => void open(card.name)}>
                        <FolderOpen size={13} />{t("Open CLI in project")}
                      </button>
                    ) : !card.inactive && (
                      <button type="button" className="secondary-button" disabled={locked || card.loading || !!card.error} onClick={() => onSignIn(card.name)}>
                        <LogIn size={13} />{t(provider === "codex" ? "Sign in with ChatGPT" : "Sign in with Google")}
                      </button>
                    )}
                    {(card.signedIn || card.inactive) && (
                      <button type="button" className="danger-button" disabled={locked} onClick={() => toggleRemoved(card, false)}>
                        <Trash2 size={13} />{t("Remove account")}
                      </button>
                    )}
                  </div>
                )}
              </footer>
            </article>
          );
        })}
      </div>
      {!viewingRemoved && drafts.length > 0 && (
        <div className="cli-pool-drafts">
          <span>{t("Sign-ins to complete")}</span>
          {drafts.map((card) => (
            <div key={card.name ?? ""} className="cli-draft-row">
              <span>{label(card.name)}</span>
              {card.error && <span role="alert">{localizeValue(card.error.replace(/^operation failed: /, ""), language)}</span>}
              <button type="button" className="secondary-button" disabled={locked || card.loading || !!card.error} onClick={() => onSignIn(card.name)}>
                <LogIn size={13} />{t(provider === "codex" ? "Sign in with ChatGPT" : "Sign in with Google")}
              </button>
              <button type="button" className="danger-button" disabled={locked} onClick={() => toggleRemoved(card, false)}>
                <Trash2 size={13} />{t("Remove profile")}
              </button>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
