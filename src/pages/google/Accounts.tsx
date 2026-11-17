import { useState } from "react";
import {
  Archive,
  Edit3,
  LayoutGrid,
  List,
  Plus,
  Search,
  Star,
  Power,
} from "lucide-react";
import {
  googleApi,
  type GoogleAccount,
  type GoogleSnapshot,
} from "../../lib/googleControl";
import {
  accountAttention,
  accountInput,
  accountName,
  Empty,
  Expiry,
  timestamp,
  useGoogleText,
  type Run,
} from "./shared";

export function Accounts({
  snapshot,
  query,
  setQuery,
  searchRef,
  onAdd,
  onEdit,
  onArchive,
  busy,
  run,
}: {
  snapshot: GoogleSnapshot;
  query: string;
  setQuery: (query: string) => void;
  searchRef?: React.RefObject<HTMLInputElement | null>;
  onAdd: () => void;
  onEdit: (account: GoogleAccount) => void;
  onArchive: (account: GoogleAccount) => void;
  busy: boolean;
  run: Run;
}) {
  const text = useGoogleText();
  const [view, setView] = useState<"grid" | "table">("grid");
  const [filter, setFilter] = useState("all");
  const [group, setGroup] = useState("");
  const [tag, setTag] = useState("");
  const [sort, setSort] = useState("name");
  const groups = [
    ...new Set(snapshot.accounts.map((a) => a.group).filter(Boolean)),
  ].sort();
  const tags = [...new Set(snapshot.accounts.flatMap((a) => a.tags))].sort();
  const accounts = snapshot.accounts
    .filter((a) => {
      if (filter === "archived" ? !a.archived : a.archived) return false;
      if (
        (filter === "enabled" && !a.enabled) ||
        (filter === "disabled" && a.enabled) ||
        (filter === "favorites" && !a.favorite) ||
        (filter === "attention" &&
          !accountAttention(a, snapshot.preferences.expirationWarningDays) &&
          !snapshot.services.some((service) =>
            service.statuses.some(
              (status) =>
                status.accountId === a.id &&
                (status.status === "login_required" ||
                  status.status === "unavailable"),
            ),
          ))
      )
        return false;
      return (
        (!group || a.group === group) &&
        (!tag || a.tags.includes(tag)) &&
        [a.name, a.email, a.subscription, a.group, a.notes, ...a.tags]
          .join(" ")
          .toLowerCase()
          .includes(query.toLowerCase())
      );
    })
    .sort((a, b) =>
      sort === "recent"
        ? (b.lastUsed ?? 0) - (a.lastUsed ?? 0)
        : sort === "expiry"
          ? (a.expirationDate ?? "9999").localeCompare(
              b.expirationDate ?? "9999",
            )
          : accountName(a).localeCompare(accountName(b)),
    );
  return (
    <>
      <div className="gc-toolbar">
        <div className="gc-search">
          <Search size={16} />
          <input
            ref={searchRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={text(
              "Search name, email, tags, notes…",
              "Ad, e-posta, etiket, not ara…",
            )}
            aria-label={text(
              "Search Google accounts",
              "Google hesaplarında ara",
            )}
          />
          <kbd>Ctrl K</kbd>
        </div>
        <button className="primary-button" onClick={onAdd} disabled={busy}>
          <Plus size={15} />
          {text("Add account", "Hesap ekle")}
        </button>
      </div>
      <div className="gc-filters">
        <select
          aria-label={text("Account filter", "Hesap filtresi")}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        >
          {[
            ["all", text("All unarchived", "Arşivlenmemiş tümü")],
            ["enabled", text("Enabled", "Etkin")],
            ["disabled", text("Disabled", "Devre dışı")],
            ["favorites", text("Favorites", "Favoriler")],
            ["attention", text("Needs attention", "İlgi gerekiyor")],
            ["archived", text("Archived", "Arşiv")],
          ].map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
        <select
          aria-label={text("Filter by group", "Gruba göre filtrele")}
          value={group}
          onChange={(e) => setGroup(e.target.value)}
        >
          <option value="">{text("All groups", "Tüm gruplar")}</option>
          {groups.map((g) => (
            <option key={g}>{g}</option>
          ))}
        </select>
        <select
          aria-label={text("Filter by tag", "Etikete göre filtrele")}
          value={tag}
          onChange={(e) => setTag(e.target.value)}
        >
          <option value="">{text("All tags", "Tüm etiketler")}</option>
          {tags.map((t) => (
            <option key={t}>{t}</option>
          ))}
        </select>
        <select
          aria-label={text("Sort accounts", "Hesapları sırala")}
          value={sort}
          onChange={(e) => setSort(e.target.value)}
        >
          <option value="name">{text("Name", "Ad")}</option>
          <option value="recent">
            {text("Recently used", "Son kullanılan")}
          </option>
          <option value="expiry">{text("Expiring first", "Önce biten")}</option>
        </select>
        <span className="gc-muted gc-result-count">
          {accounts.length} {text("accounts", "hesap")}
        </span>
        <div className="gc-view-toggle">
          <button
            className={`icon-button ${view === "grid" ? "is-active" : ""}`}
            onClick={() => setView("grid")}
            aria-label={text("Account cards", "Hesap kartları")}
            aria-pressed={view === "grid"}
          >
            <LayoutGrid size={16} />
          </button>
          <button
            className={`icon-button ${view === "table" ? "is-active" : ""}`}
            onClick={() => setView("table")}
            aria-label={text("Account table", "Hesap tablosu")}
            aria-pressed={view === "table"}
          >
            <List size={16} />
          </button>
        </div>
      </div>
      {!accounts.length ? (
        <Empty>
          <p>
            {text(
              "No accounts match this view.",
              "Bu görünüme uyan hesap yok.",
            )}
          </p>
          <button className="secondary-button" onClick={onAdd}>
            <Plus size={15} />
            {text("Add a Google account", "Google hesabı ekle")}
          </button>
        </Empty>
      ) : view === "grid" ? (
        <div className="gc-account-grid">
          {accounts.map((account) => (
            <article
              className="glass-panel gc-account-card"
              key={account.id}
              style={{ borderTopColor: account.color }}
            >
              <div className="gc-card-heading">
                <span className="gc-avatar" style={{ color: account.color }}>
                  {accountName(account).slice(0, 2).toUpperCase()}
                </span>
                <div>
                  <h3>{accountName(account)}</h3>
                  <p>{account.email}</p>
                </div>
                <button
                  className={`icon-button ${account.favorite ? "gc-favorite" : ""}`}
                  aria-label={text("Toggle favorite", "Favoriyi değiştir")}
                  aria-pressed={account.favorite}
                  disabled={busy}
                  onClick={() =>
                    void run(() =>
                      googleApi.saveAccount({
                        ...accountInput(account),
                        favorite: !account.favorite,
                      }),
                    )
                  }
                >
                  <Star
                    size={16}
                    fill={account.favorite ? "currentColor" : "none"}
                  />
                </button>
              </div>
              <div className="gc-account-meta">
                <strong>
                  {account.subscription ||
                    text("No subscription label", "Abonelik etiketi yok")}
                </strong>
                <Expiry account={account} />
                <span>
                  {text("Last used", "Son kullanım")}:{" "}
                  {timestamp(account.lastUsed, text)}
                </span>
              </div>
              <div className="gc-tags">
                <span
                  className={`gc-badge${account.enabled && !account.archived ? " gc-badge--good" : ""}`}
                >
                  {account.archived
                    ? text("Archived", "Arşiv")
                    : account.enabled
                      ? text("Enabled", "Etkin")
                      : text("Disabled", "Devre dışı")}
                </span>
                {account.group && (
                  <span className="gc-badge">{account.group}</span>
                )}
                {account.tags.map((t) => (
                  <span key={t} className="gc-badge">
                    #{t}
                  </span>
                ))}
              </div>
              {account.notes && (
                <p className="gc-account-notes">{account.notes}</p>
              )}
              <div className="gc-card-footer">
                <span>
                  {
                    snapshot.projects.filter((p) =>
                      p.assignments.some((a) => a.accountId === account.id),
                    ).length
                  }{" "}
                  {text("projects", "proje")}
                </span>
                <AccountActions
                  account={account}
                  onEdit={onEdit}
                  onArchive={onArchive}
                  run={run}
                  busy={busy}
                />
              </div>
            </article>
          ))}
        </div>
      ) : (
        <div className="glass-panel gc-table-scroll">
          <table className="gc-table">
            <thead>
              <tr>
                {[
                  text("Account", "Hesap"),
                  text("Subscription / expiration", "Abonelik / bitiş"),
                  text("Group / tags", "Grup / etiket"),
                  text("Status", "Durum"),
                  text("Actions", "İşlemler"),
                ].map((h) => (
                  <th key={h}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {accounts.map((a) => (
                <tr key={a.id}>
                  <td>
                    <strong>
                      {a.favorite && <Star size={12} className="gc-favorite" />}{" "}
                      {accountName(a)}
                    </strong>
                    <small>{a.email}</small>
                    <small>{timestamp(a.lastUsed, text)}</small>
                  </td>
                  <td>
                    {a.subscription || "—"}
                    <small>
                      <Expiry account={a} />
                    </small>
                  </td>
                  <td>
                    {a.group || "—"}
                    <small>{a.tags.join(", ")}</small>
                  </td>
                  <td>
                    {a.archived
                      ? text("Archived", "Arşiv")
                      : a.enabled
                        ? text("Enabled", "Etkin")
                        : text("Disabled", "Devre dışı")}
                  </td>
                  <td>
                    <AccountActions
                      account={a}
                      onEdit={onEdit}
                      onArchive={onArchive}
                      run={run}
                      busy={busy}
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
function AccountActions({
  account,
  onEdit,
  onArchive,
  run,
  busy,
}: {
  account: GoogleAccount;
  onEdit: (a: GoogleAccount) => void;
  onArchive: (a: GoogleAccount) => void;
  run: Run;
  busy: boolean;
}) {
  const text = useGoogleText();
  return (
    <div className="gc-inline-actions">
      <button
        className="icon-button"
        aria-label={text("Edit account", "Hesabı düzenle")}
        title={text("Edit / restore account", "Hesabı düzenle / geri al")}
        onClick={() => onEdit(account)}
        disabled={busy}
      >
        <Edit3 size={14} />
      </button>
      <button
        className="icon-button"
        aria-label={
          account.enabled
            ? text("Disable account", "Hesabı devre dışı bırak")
            : text("Enable account", "Hesabı etkinleştir")
        }
        title={
          account.enabled
            ? text("Disable", "Devre dışı bırak")
            : text("Enable", "Etkinleştir")
        }
        aria-pressed={account.enabled}
        disabled={busy || account.archived}
        onClick={() =>
          void run(() =>
            googleApi.saveAccount({
              ...accountInput(account),
              enabled: !account.enabled,
            }),
          )
        }
      >
        <Power size={14} />
      </button>
      <button
        className="icon-button"
        aria-label={text("Archive account", "Hesabı arşivle")}
        title={text("Archive management entry", "Yönetim kaydını arşivle")}
        onClick={() => onArchive(account)}
        disabled={busy || account.archived}
      >
        <Archive size={14} />
      </button>
    </div>
  );
}
