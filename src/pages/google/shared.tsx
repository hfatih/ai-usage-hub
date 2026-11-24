import { useEffect, useRef, type ReactNode } from "react";
import { X } from "lucide-react";
import { useTranslation } from "../../i18n";
import type {
  GoogleAccount,
  GoogleAccountInput,
  GoogleProject,
  GoogleProjectInput,
  GoogleSnapshot,
} from "../../lib/googleControl";

export type Text = (en: string, tr: string) => string;
export type Run = (
  action: () => Promise<GoogleSnapshot>,
  message?: string,
) => Promise<boolean>;
export type Tab =
  | "overview"
  | "accounts"
  | "services"
  | "projects"
  | "usage"
  | "activity"
  | "settings";
export function useGoogleText(): Text {
  const { language } = useTranslation();
  return (en, tr) => (language === "tr" ? tr : en);
}
export function accountInput(account: GoogleAccount): GoogleAccountInput {
  const {
    id,
    email,
    name,
    color,
    subscription,
    activationDate,
    expirationDate,
    enabled,
    archived,
    favorite,
    group,
    tags,
    notes,
  } = account;
  return {
    id,
    email,
    name,
    color,
    subscription,
    activationDate,
    expirationDate,
    enabled,
    archived,
    favorite,
    group,
    tags: [...tags],
    notes,
  };
}
export function projectInput(project: GoogleProject): GoogleProjectInput {
  const { id, name, description, notes, status, assignments } = project;
  return {
    id,
    name,
    description,
    notes,
    status,
    assignments: assignments.map(({ accountId, role, services }) => ({
      accountId,
      role,
      services: [...services],
    })),
  };
}
export function expirationDays(
  account: GoogleAccount,
  now = new Date(),
): number | null {
  if (!account.expirationDate) return null;
  const [year, month, day] = account.expirationDate.split("-").map(Number);
  const expiry = Date.UTC(year, month - 1, day);
  const today = Date.UTC(now.getFullYear(), now.getMonth(), now.getDate());
  return (expiry - today) / 86400000;
}
export function accountAttention(
  account: GoogleAccount,
  days: number,
): boolean {
  const remaining = expirationDays(account);
  return (
    !account.archived &&
    (!account.enabled || (remaining !== null && remaining <= days))
  );
}
export function accountName(account?: GoogleAccount): string {
  return account ? account.name || account.email : "—";
}
export function timestamp(value: number | null, text: Text): string {
  return value
    ? new Date(value * 1000).toLocaleString(text("en-US", "tr-TR"), {
        dateStyle: "medium",
        timeStyle: "short",
      })
    : text("Not recorded", "Kaydedilmedi");
}
export function Expiry({ account }: { account: GoogleAccount }) {
  const text = useGoogleText();
  const days = expirationDays(account);
  if (days === null)
    return (
      <span className="gc-muted">
        {text("No expiration set", "Bitiş tarihi yok")}
      </span>
    );
  return (
    <span className={days <= 7 ? "gc-warning" : "gc-muted"}>
      {days < 0
        ? text(
            `Expired ${Math.abs(days)} days ago`,
            `${Math.abs(days)} gün önce sona erdi`,
          )
        : days === 0
          ? text("Expires today", "Bugün sona eriyor")
          : text(`${days} days left`, `${days} gün kaldı`)}{" "}
      <small>· {account.expirationDate}</small>
    </span>
  );
}
export function Empty({ children }: { children: ReactNode }) {
  return <div className="glass-panel gc-empty">{children}</div>;
}
export function Field({
  label,
  children,
  wide = false,
}: {
  label: string;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <label className={`gc-field${wide ? " gc-field--wide" : ""}`}>
      <span>{label}</span>
      {children}
    </label>
  );
}
export function Modal({
  title,
  children,
  onClose,
  busy,
  error,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  busy: boolean;
  error?: string | null;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const text = useGoogleText();
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const target = ref.current;
    const focusables = () => [
      ...(target?.querySelectorAll<HTMLElement>(
        'button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex="0"]',
      ) ?? []),
    ];
    focusables()
      .find((el) => el.tagName !== "BUTTON")
      ?.focus();
    function key(event: KeyboardEvent) {
      if (event.key !== "Tab") return;
      const items = focusables();
      if (!items.length) {
        event.preventDefault();
        return;
      }
      const first = items[0],
        last = items[items.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }
    target?.addEventListener("keydown", key);
    return () => {
      target?.removeEventListener("keydown", key);
      previous?.focus();
    };
  }, []);
  return (
    <div
      className="gc-modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <div
        className="glass-panel gc-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="gc-modal-title"
        ref={ref}
      >
        <header>
          <h2 id="gc-modal-title">{title}</h2>
          <button
            className="icon-button"
            aria-label={text("Close", "Kapat")}
            disabled={busy}
            onClick={onClose}
          >
            <X size={17} />
          </button>
        </header>
        {error && (
          <div className="gc-notice gc-notice--error" role="alert">
            {error}
          </div>
        )}
        {children}
      </div>
    </div>
  );
}
export function AccountSelect({
  accounts,
  value,
  onChange,
  label,
  disabled = false,
  allowBlank = true,
}: {
  accounts: GoogleAccount[];
  value: string;
  onChange: (id: string) => void;
  label: string;
  disabled?: boolean;
  allowBlank?: boolean;
}) {
  const text = useGoogleText();
  return (
    <select
      aria-label={label}
      value={value}
      onChange={(event) => onChange(event.target.value)}
      disabled={disabled}
      required
    >
      {allowBlank && (
        <option value="">{text("Select account", "Hesap seç")}</option>
      )}
      {accounts.map((account) => (
        <option key={account.id} value={account.id}>
          {accountName(account)} · {account.email}
          {account.archived
            ? text(" (archived)", " (arşiv)")
            : !account.enabled
              ? text(" (disabled)", " (devre dışı)")
              : ""}
        </option>
      ))}
    </select>
  );
}
