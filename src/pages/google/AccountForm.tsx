import { useState } from "react";
import type {
  GoogleAccount,
  GoogleAccountInput,
} from "../../lib/googleControl";
import { accountInput, Field, useGoogleText } from "./shared";

const blankAccount: GoogleAccountInput = {
  email: "",
  name: "",
  color: "#dba8c2",
  subscription: "",
  activationDate: null,
  expirationDate: null,
  enabled: true,
  archived: false,
  favorite: false,
  group: "",
  tags: [],
  notes: "",
};
export function AccountForm({
  account,
  busy,
  onSave,
  onCancel,
}: {
  account: GoogleAccount | null;
  busy: boolean;
  onSave: (account: GoogleAccountInput) => Promise<void>;
  onCancel: () => void;
}) {
  const text = useGoogleText();
  const [form, setForm] = useState<GoogleAccountInput>(
    account ? accountInput(account) : { ...blankAccount },
  );
  const [tags, setTags] = useState(account?.tags.join(", ") ?? "");
  const update = <K extends keyof GoogleAccountInput>(
    key: K,
    value: GoogleAccountInput[K],
  ) => setForm((current) => ({ ...current, [key]: value }));
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        void onSave({
          ...form,
          email: form.email.trim(),
          name: form.name.trim(),
          tags: [
            ...new Set(
              tags
                .split(",")
                .map((tag) => tag.trim())
                .filter(Boolean),
            ),
          ],
        });
      }}
    >
      <div className="gc-form-grid">
        <Field label={text("Account name", "Hesap adı")}>
          <input
            autoFocus
            required
            maxLength={120}
            value={form.name}
            placeholder="JIO-01"
            onChange={(e) => update("name", e.target.value)}
          />
        </Field>
        <Field label={text("Google email", "Google e-posta")}>
          <input
            type="email"
            readOnly={Boolean(account)}
            aria-describedby={account ? "gc-email-hint" : undefined}
            required
            maxLength={254}
            value={form.email}
            onChange={(e) => update("email", e.target.value)}
          />
          {account && (
            <small id="gc-email-hint">
              {text(
                "Add another account to use a different email.",
                "Farklı e-posta için başka bir hesap ekle.",
              )}
            </small>
          )}
        </Field>
        <Field label={text("Account color", "Hesap rengi")}>
          <input
            className="gc-color-input"
            type="color"
            value={form.color}
            onChange={(e) => update("color", e.target.value)}
          />
        </Field>
        <Field
          label={text("Subscription (your label)", "Abonelik (kendi etiketin)")}
        >
          <input
            maxLength={120}
            value={form.subscription}
            placeholder="Google AI Pro · Jio"
            onChange={(e) => update("subscription", e.target.value)}
          />
        </Field>
        <Field label={text("Activation date", "Başlangıç tarihi")}>
          <input
            type="date"
            value={form.activationDate ?? ""}
            max={form.expirationDate ?? undefined}
            onChange={(e) => update("activationDate", e.target.value || null)}
          />
        </Field>
        <Field label={text("Expiration date", "Bitiş tarihi")}>
          <input
            type="date"
            value={form.expirationDate ?? ""}
            min={form.activationDate ?? undefined}
            onChange={(e) => update("expirationDate", e.target.value || null)}
          />
        </Field>
        <Field label={text("Purpose / group", "Amaç / grup")}>
          <input
            maxLength={120}
            value={form.group}
            onChange={(e) => update("group", e.target.value)}
          />
        </Field>
        <Field
          label={text("Tags (comma separated)", "Etiketler (virgülle ayır)")}
        >
          <input
            value={tags}
            maxLength={1000}
            onChange={(e) => setTags(e.target.value)}
          />
        </Field>
        <Field label={text("Personal notes", "Kişisel notlar")} wide>
          <textarea
            rows={3}
            maxLength={10000}
            value={form.notes}
            onChange={(e) => update("notes", e.target.value)}
          />
        </Field>
      </div>
      <div className="gc-check-row">
        <label>
          <input
            type="checkbox"
            checked={form.enabled}
            onChange={(e) => update("enabled", e.target.checked)}
          />
          {text("Enabled", "Etkin")}
        </label>
        <label>
          <input
            type="checkbox"
            checked={form.favorite}
            onChange={(e) => update("favorite", e.target.checked)}
          />
          {text("Favorite", "Favori")}
        </label>
        {account && (
          <label>
            <input
              type="checkbox"
              checked={form.archived}
              onChange={(e) => update("archived", e.target.checked)}
            />
            {text("Archived", "Arşivlendi")}
          </label>
        )}
      </div>
      <p className="gc-hint">
        {text(
          "These are management details you enter. Subscription benefits and Google sign-in are verified separately in each account's browser.",
          "Bunlar senin girdiğin yönetim bilgileridir. Abonelik hakları ve Google oturumu her hesabın tarayıcısında ayrıca doğrulanır.",
        )}
      </p>
      <footer className="gc-form-actions">
        <button
          type="button"
          className="secondary-button"
          onClick={onCancel}
          disabled={busy}
        >
          {text("Cancel", "Vazgeç")}
        </button>
        <button className="primary-button" disabled={busy}>
          {busy
            ? text("Saving…", "Kaydediliyor…")
            : text("Save account", "Hesabı kaydet")}
        </button>
      </footer>
    </form>
  );
}
