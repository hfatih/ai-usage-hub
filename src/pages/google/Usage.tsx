import { useState } from "react";
import { Database, Plus, RefreshCw, Trash2 } from "lucide-react";
import {
  googleApi,
  type GoogleSnapshot,
  type GoogleUsageInput,
} from "../../lib/googleControl";
import { useHub } from "../../stores/hub";
import {
  AccountSelect,
  accountName,
  Empty,
  Field,
  timestamp,
  useGoogleText,
  type Run,
} from "./shared";

export function Usage({
  snapshot,
  busy,
  run,
  onDelete,
}: {
  snapshot: GoogleSnapshot;
  busy: boolean;
  run: Run;
  onDelete: (id: number) => void;
}) {
  const text = useGoogleText();
  const [accountFilter, setAccountFilter] = useState("");
  const [serviceFilter, setServiceFilter] = useState("");
  const [sourceFilter, setSourceFilter] = useState("");
  const [showForm, setShowForm] = useState(false);
  const [form, setForm] = useState<GoogleUsageInput>({
    accountId: "",
    serviceId: snapshot.services[0]?.id ?? "gemini",
    metric: "",
    value: 0,
    maxValue: null,
    unit: "",
  });
  const [recordedAt, setRecordedAt] = useState("");
  const records = snapshot.usage
    .filter(
      (r) =>
        (!accountFilter || r.accountId === accountFilter) &&
        (!serviceFilter || r.serviceId === serviceFilter) &&
        (!sourceFilter || r.source === sourceFilter),
    )
    .sort((a, b) => b.recordedAt - a.recordedAt);
  const nativeAccounts = snapshot.accounts.filter(
    (a) =>
      !a.archived &&
      a.antigravityId &&
      (!accountFilter || a.id === accountFilter),
  );
  const suggestions: Record<string, string[]> = {
    flow: [text("Credits remaining", "Kalan kredi")],
    gemini: [text("Requests used", "Kullanılan istek")],
    notebooklm: [
      text("Notebook count", "Not defteri sayısı"),
      text("Source count", "Kaynak sayısı"),
    ],
    drive: [text("Storage used", "Kullanılan depolama")],
    antigravity: [text("Requests remaining", "Kalan istek")],
  };
  async function saveRecord() {
    const saved = await run(
      () =>
        googleApi.addUsage({
          ...form,
          metric: form.metric.trim(),
          unit: form.unit.trim(),
          recordedAt: recordedAt
            ? Math.floor(new Date(recordedAt).getTime() / 1000)
            : undefined,
        }),
      text("Manual measurement recorded.", "Manuel ölçüm kaydedildi."),
    );
    if (saved) {
      setShowForm(false);
      setForm({ ...form, metric: "", value: 0, maxValue: null, unit: "" });
      setRecordedAt("");
    }
  }
  return (
    <>
      <div className="gc-source-explainer">
        <Database size={20} />
        <div>
          <strong>
            {text(
              "Every number belongs to one account and one source.",
              "Her sayı tek bir hesaba ve kaynağa aittir.",
            )}
          </strong>
          <p>
            {text(
              "Antigravity data comes from the existing native connection. Flow, Gemini, NotebookLM, Drive and other service measurements can be entered manually; no unsupported quotas are estimated.",
              "Antigravity verileri mevcut yerel bağlantıdan gelir. Flow, Gemini, NotebookLM, Drive ve diğer hizmetlerin ölçümleri manuel girilebilir; desteklenmeyen kotalar tahmin edilmez.",
            )}
          </p>
        </div>
      </div>
      <div className="gc-section-heading">
        <div>
          <h2>
            {text("Antigravity · native source", "Antigravity · yerel kaynak")}
          </h2>
          <p>
            {text(
              "Last known quota for each linked account",
              "Bağlı her hesabın son bilinen kotası",
            )}
          </p>
        </div>
        <button
          className="secondary-button"
          disabled={busy}
          onClick={() =>
            void run(
              async () => {
                await useHub.getState().refreshAllAntigravityAccounts();
                return googleApi.snapshot();
              },
              text(
                "Native Antigravity quotas refreshed.",
                "Yerel Antigravity kotaları yenilendi.",
              ),
            )
          }
        >
          <RefreshCw size={14} />
          {text("Refresh native quotas", "Yerel kotaları yenile")}
        </button>
      </div>
      {!nativeAccounts.length ? (
        <Empty>
          {text(
            "No linked Antigravity account in this view. Add a sign-in in Accounts → Antigravity; its metadata will appear here.",
            "Bu görünümde bağlı Antigravity hesabı yok. Hesaplar → Antigravity bölümünde giriş ekle; yönetim bilgileri burada görünür.",
          )}
        </Empty>
      ) : (
        <div className="gc-native-grid">
          {nativeAccounts.map((account) => (
            <article className="glass-panel gc-native-card" key={account.id}>
              <div>
                <strong>{accountName(account)}</strong>
                <small>{account.email}</small>
              </div>
              <span className="gc-native-value">
                {account.quotaRemaining === null
                  ? "—"
                  : `${Math.round(account.quotaRemaining)}%`}
                <small>
                  {text("remaining · native source", "kalan · yerel kaynak")}
                </small>
              </span>
              <small>
                {text("Last synchronized", "Son eşitleme")}:{" "}
                {timestamp(account.quotaUpdatedAt, text)}
              </small>
              {account.quotaDetails && (
                <details>
                  <summary>
                    {text(
                      "Reported model details",
                      "Bildirilen model ayrıntıları",
                    )}
                  </summary>
                  <pre>{prettyDetails(account.quotaDetails)}</pre>
                </details>
              )}
              {account.quotaRemaining === null && (
                <small className="gc-muted">
                  {text(
                    "No quota value reported. Refresh the native connection.",
                    "Kota değeri bildirilmedi. Yerel bağlantıyı yenile.",
                  )}
                </small>
              )}
            </article>
          ))}
        </div>
      )}
      <div className="gc-section-heading">
        <div>
          <h2>{text("Usage measurements", "Kullanım ölçümleri")}</h2>
          <p>
            {text(
              "Manual entries remain labeled and keep their measurement history.",
              "Manuel girişler etiketlenir ve ölçüm geçmişleri korunur.",
            )}
          </p>
        </div>
        <button
          className="primary-button"
          disabled={busy || !snapshot.accounts.some((a) => !a.archived)}
          onClick={() => setShowForm(!showForm)}
          aria-expanded={showForm}
        >
          <Plus size={15} />
          {text("Record manual value", "Manuel değer kaydet")}
        </button>
      </div>
      {showForm && (
        <form
          className="glass-panel gc-inline-form"
          onKeyDown={(event) => {
            if (event.key === "Escape" && !busy) setShowForm(false);
          }}
          onSubmit={(e) => {
            e.preventDefault();
            void saveRecord();
          }}
        >
          <h3>{text("New manual measurement", "Yeni manuel ölçüm")}</h3>
          <div className="gc-form-grid">
            <Field label={text("Account", "Hesap")}>
              <AccountSelect
                accounts={snapshot.accounts.filter((a) => !a.archived)}
                value={form.accountId}
                onChange={(id) => setForm({ ...form, accountId: id })}
                label={text("Measurement account", "Ölçüm hesabı")}
              />
            </Field>
            <Field label={text("Service", "Hizmet")}>
              <select
                value={form.serviceId}
                onChange={(e) =>
                  setForm({ ...form, serviceId: e.target.value })
                }
              >
                {snapshot.services.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label={text("Metric name", "Ölçüm adı")}>
              <input
                required
                maxLength={160}
                list="gc-metric-suggestions"
                value={form.metric}
                onChange={(e) => setForm({ ...form, metric: e.target.value })}
              />
              <datalist id="gc-metric-suggestions">
                {(suggestions[form.serviceId] ?? []).map((metric) => (
                  <option key={metric} value={metric} />
                ))}
              </datalist>
            </Field>
            <Field label={text("Value", "Değer")}>
              <input
                type="number"
                min="0"
                step="any"
                required
                value={form.value}
                onChange={(e) =>
                  setForm({ ...form, value: e.target.valueAsNumber })
                }
              />
            </Field>
            <Field label={text("Limit (optional)", "Üst sınır (isteğe bağlı)")}>
              <input
                type="number"
                min="0.000001"
                step="any"
                value={form.maxValue ?? ""}
                onChange={(e) =>
                  setForm({
                    ...form,
                    maxValue:
                      e.target.value === "" ? null : e.target.valueAsNumber,
                  })
                }
              />
            </Field>
            <Field label={text("Unit", "Birim")}>
              <input
                required
                maxLength={40}
                value={form.unit}
                placeholder={text(
                  "credits, GB, notebooks…",
                  "kredi, GB, not defteri…",
                )}
                onChange={(e) => setForm({ ...form, unit: e.target.value })}
              />
            </Field>
            <Field
              label={text(
                "Measured at (blank = now)",
                "Ölçüm zamanı (boş = şimdi)",
              )}
              wide
            >
              <input
                type="datetime-local"
                value={recordedAt}
                onChange={(e) => setRecordedAt(e.target.value)}
              />
            </Field>
          </div>
          <p className="gc-hint">
            {text(
              "Source: manual. Enter the value shown in this account's official service page. This does not synchronize with Google.",
              "Kaynak: manuel. Bu hesabın resmî hizmet sayfasında gördüğün değeri gir. Bu işlem Google ile eşitleme yapmaz.",
            )}
          </p>
          <div className="gc-form-actions">
            <button
              type="button"
              className="secondary-button"
              disabled={busy}
              onClick={() => setShowForm(false)}
            >
              {text("Cancel", "Vazgeç")}
            </button>
            <button className="primary-button" disabled={busy}>
              {text("Save manual measurement", "Manuel ölçümü kaydet")}
            </button>
          </div>
        </form>
      )}
      <div className="gc-filters">
        <select
          aria-label={text("Usage account filter", "Kullanım hesap filtresi")}
          value={accountFilter}
          onChange={(e) => setAccountFilter(e.target.value)}
        >
          <option value="">{text("All accounts", "Tüm hesaplar")}</option>
          {snapshot.accounts.map((a) => (
            <option key={a.id} value={a.id}>
              {accountName(a)}
            </option>
          ))}
        </select>
        <select
          aria-label={text("Usage service filter", "Kullanım hizmet filtresi")}
          value={serviceFilter}
          onChange={(e) => setServiceFilter(e.target.value)}
        >
          <option value="">{text("All services", "Tüm hizmetler")}</option>
          {snapshot.services.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
        <select
          aria-label={text("Usage source filter", "Kullanım kaynak filtresi")}
          value={sourceFilter}
          onChange={(e) => setSourceFilter(e.target.value)}
        >
          <option value="">{text("All sources", "Tüm kaynaklar")}</option>
          <option value="manual">{text("Manual", "Manuel")}</option>
          <option value="antigravity">
            {text("Native Antigravity", "Yerel Antigravity")}
          </option>
        </select>
        <span className="gc-muted">
          {records.length} {text("measurements", "ölçüm")}
        </span>
      </div>
      {!records.length ? (
        <Empty>
          {text(
            "No usage measurements in this view. Record a value from the selected account's service page.",
            "Bu görünümde kullanım ölçümü yok. Seçilen hesabın hizmet sayfasından bir değer kaydet.",
          )}
        </Empty>
      ) : (
        <div className="glass-panel gc-table-scroll">
          <table className="gc-table">
            <thead>
              <tr>
                {[
                  text("Account / service", "Hesap / hizmet"),
                  text("Metric", "Ölçüm"),
                  text("Value", "Değer"),
                  text("Source / measured at", "Kaynak / ölçüm zamanı"),
                  text("Actions", "İşlemler"),
                ].map((h) => (
                  <th key={h}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {records.map((record) => (
                <tr key={record.id}>
                  <td>
                    <strong>
                      {accountName(
                        snapshot.accounts.find(
                          (a) => a.id === record.accountId,
                        ),
                      )}
                    </strong>
                    <small>
                      {snapshot.services.find((s) => s.id === record.serviceId)
                        ?.name ?? record.serviceId}
                    </small>
                  </td>
                  <td>{record.metric}</td>
                  <td>
                    <strong>
                      {record.value.toLocaleString(text("en-US", "tr-TR"))}
                      {record.maxValue !== null
                        ? ` / ${record.maxValue}`
                        : ""}{" "}
                      {record.unit}
                    </strong>
                    {record.maxValue !== null && record.maxValue > 0 && (
                      <progress
                        max={record.maxValue}
                        value={record.value}
                        aria-label={record.metric}
                      />
                    )}
                  </td>
                  <td>
                    <span
                      className={`gc-badge${record.source === "manual" ? " gc-badge--manual" : " gc-badge--good"}`}
                    >
                      {record.source === "manual"
                        ? text("Manual", "Manuel")
                        : text("Native Antigravity", "Yerel Antigravity")}
                    </span>
                    <small>{timestamp(record.recordedAt, text)}</small>
                  </td>
                  <td>
                    {record.source === "manual" && (
                      <button
                        className="icon-button"
                        disabled={busy}
                        aria-label={text(
                          "Delete manual measurement",
                          "Manuel ölçümü sil",
                        )}
                        onClick={() => onDelete(record.id)}
                      >
                        <Trash2 size={15} />
                      </button>
                    )}
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
function prettyDetails(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}
