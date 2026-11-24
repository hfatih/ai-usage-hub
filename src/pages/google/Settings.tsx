import { useEffect, useState } from "react";
import {
  Copy,
  DatabaseBackup,
  Download,
  FileUp,
  RefreshCw,
  Save,
  ShieldCheck,
} from "lucide-react";
import {
  googleApi,
  type GooglePreferences,
  type GoogleSnapshot,
} from "../../lib/googleControl";
import { Field, timestamp, useGoogleText, type Run } from "./shared";

export function Settings({
  snapshot,
  busy,
  run,
  onRestore,
  notify,
}: {
  snapshot: GoogleSnapshot;
  busy: boolean;
  run: Run;
  onRestore: (path: string) => void;
  notify: (message: string, error?: boolean) => void;
}) {
  const text = useGoogleText();
  const [preferences, setPreferences] = useState<GooglePreferences>({
    ...snapshot.preferences,
  });
  const [format, setFormat] = useState<"json" | "csv">("json");
  const [importText, setImportText] = useState("");
  const [transferBusy, setTransferBusy] = useState(false);
  const locked = busy || transferBusy;
  useEffect(
    () => setPreferences({ ...snapshot.preferences }),
    [
      snapshot.preferences.notificationsEnabled,
      snapshot.preferences.expirationWarningDays,
      snapshot.preferences.autoBackup,
      snapshot.preferences.backupRetention,
    ],
  );
  async function exportData(copy = false) {
    setTransferBusy(true);
    try {
      const value = await googleApi.exportMetadata(format);
      if (copy) {
        await navigator.clipboard.writeText(value);
        notify(
          text(
            "Metadata copied to clipboard.",
            "Yönetim bilgileri panoya kopyalandı.",
          ),
        );
      } else {
        const blob = new Blob([value], {
          type:
            format === "json"
              ? "application/json;charset=utf-8"
              : "text/csv;charset=utf-8",
        });
        const url = URL.createObjectURL(blob);
        const link = document.createElement("a");
        link.href = url;
        link.download = `google-control-metadata-${new Date().toISOString().slice(0, 10)}.${format}`;
        document.body.appendChild(link);
        link.click();
        link.remove();
        setTimeout(() => URL.revokeObjectURL(url), 30000);
        notify(
          text(
            "Metadata export downloaded.",
            "Yönetim bilgileri dışa aktarıldı.",
          ),
        );
      }
    } catch (error) {
      notify(String(error), true);
    } finally {
      setTransferBusy(false);
    }
  }
  async function readFile(file: File | undefined) {
    if (!file) return;
    if (file.size > 10 * 1024 * 1024) {
      notify(
        text(
          "Choose a metadata file smaller than 10 MB.",
          "10 MB'dan küçük yönetim bilgisi dosyası seç.",
        ),
        true,
      );
      return;
    }
    try {
      setImportText(await file.text());
      setFormat(file.name.toLowerCase().endsWith(".csv") ? "csv" : "json");
    } catch (error) {
      notify(String(error), true);
    }
  }
  async function chooseRestore() {
    setTransferBusy(true);
    try {
      const path = await googleApi.chooseBackup();
      if (path) onRestore(path);
    } catch (error) {
      notify(String(error), true);
    } finally {
      setTransferBusy(false);
    }
  }
  return (
    <div className="gc-settings">
      <section className="glass-panel gc-settings-card">
        <div className="gc-card-heading">
          <Save size={20} />
          <div>
            <h2>{text("Preferences", "Tercihler")}</h2>
            <p>
              {text(
                "Subscription reminders and local backups",
                "Abonelik hatırlatmaları ve yerel yedekler",
              )}
            </p>
          </div>
        </div>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void run(
              () => googleApi.savePreferences(preferences),
              text(
                "Google control preferences saved.",
                "Google kontrol tercihleri kaydedildi.",
              ),
            );
          }}
        >
          <div className="gc-settings-toggles">
            <label>
              <input
                type="checkbox"
                checked={preferences.notificationsEnabled}
                onChange={(e) =>
                  setPreferences({
                    ...preferences,
                    notificationsEnabled: e.target.checked,
                  })
                }
              />
              <div>
                <strong>
                  {text("Expiration notifications", "Bitiş bildirimleri")}
                </strong>
                <small>
                  {text(
                    "Show subscription expiration alerts in the Hub.",
                    "Hub içinde abonelik bitiş uyarıları göster.",
                  )}
                </small>
              </div>
            </label>
            <label>
              <input
                type="checkbox"
                checked={preferences.autoBackup}
                onChange={(e) =>
                  setPreferences({
                    ...preferences,
                    autoBackup: e.target.checked,
                  })
                }
              />
              <div>
                <strong>
                  {text("Automatic local backup", "Otomatik yerel yedek")}
                </strong>
                <small>
                  {text(
                    "Create a daily database backup while the Hub is running.",
                    "Hub çalışırken günlük veritabanı yedeği oluştur.",
                  )}
                </small>
              </div>
            </label>
          </div>
          <div className="gc-form-grid">
            <Field
              label={text(
                "Warn before expiration (days)",
                "Bitişten önce uyar (gün)",
              )}
            >
              <input
                type="number"
                required
                min={1}
                max={365}
                value={preferences.expirationWarningDays}
                onChange={(e) =>
                  setPreferences({
                    ...preferences,
                    expirationWarningDays: e.target.valueAsNumber,
                  })
                }
              />
            </Field>
            <Field
              label={text(
                "Automatic backup retention (files)",
                "Otomatik yedek saklama (dosya)",
              )}
            >
              <input
                type="number"
                required
                min={1}
                max={100}
                value={preferences.backupRetention}
                onChange={(e) =>
                  setPreferences({
                    ...preferences,
                    backupRetention: e.target.valueAsNumber,
                  })
                }
              />
            </Field>
          </div>
          <footer className="gc-form-actions">
            <button className="primary-button" disabled={locked}>
              <Save size={15} />
              {text("Save preferences", "Tercihleri kaydet")}
            </button>
          </footer>
        </form>
      </section>
      <section className="glass-panel gc-settings-card">
        <div className="gc-card-heading">
          <Download size={20} />
          <div>
            <h2>
              {text("Import and export metadata", "Yönetim bilgilerini aktar")}
            </h2>
            <p>
              {text(
                "Portable account management information",
                "Taşınabilir hesap yönetim bilgileri",
              )}
            </p>
          </div>
        </div>
        <p className="gc-hint">
          {text(
            "JSON and CSV exports include management metadata. Passwords, provider tokens, browser cookies and Google sign-ins are excluded.",
            "JSON ve CSV çıktıları yönetim bilgilerini içerir. Parolalar, sağlayıcı tokenları, tarayıcı çerezleri ve Google oturumları dahil edilmez.",
          )}
        </p>
        <div className="gc-toolbar">
          <select
            aria-label={text("Metadata format", "Aktarım biçimi")}
            value={format}
            onChange={(e) => setFormat(e.target.value as "json" | "csv")}
            disabled={locked}
          >
            <option value="json">JSON</option>
            <option value="csv">CSV</option>
          </select>
          <button
            className="secondary-button"
            disabled={locked}
            onClick={() => void exportData()}
          >
            <Download size={15} />
            {text("Download metadata", "Bilgileri indir")}
          </button>
          <button
            className="secondary-button"
            disabled={locked}
            onClick={() => void exportData(true)}
          >
            <Copy size={15} />
            {text("Copy", "Kopyala")}
          </button>
        </div>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void run(
              () => googleApi.importMetadata(importText, format),
              text("Metadata imported.", "Yönetim bilgileri içe aktarıldı."),
            ).then((ok) => {
              if (ok) setImportText("");
            });
          }}
        >
          <Field
            label={text(
              "Choose a JSON or CSV metadata file",
              "JSON veya CSV yönetim bilgisi dosyası seç",
            )}
          >
            <input
              type="file"
              accept=".json,.csv,application/json,text/csv"
              disabled={locked}
              onChange={(e) => {
                void readFile(e.target.files?.[0]);
                e.target.value = "";
              }}
            />
          </Field>
          <Field
            label={text(
              "Or paste metadata",
              "Veya yönetim bilgilerini yapıştır",
            )}
          >
            <textarea
              rows={6}
              maxLength={10000000}
              value={importText}
              onChange={(e) => setImportText(e.target.value)}
              spellCheck={false}
              placeholder={text(
                "Paste exported JSON or CSV here…",
                "Dışa aktarılan JSON veya CSV içeriğini buraya yapıştır…",
              )}
            />
          </Field>
          <p className="gc-hint">
            {text(
              "Import validates the entire file before applying metadata changes. Google sign-in is completed separately in each account's browser.",
              "İçe aktarım, bilgileri uygulamadan önce dosyanın tamamını doğrular. Google girişi her hesabın tarayıcısında ayrıca tamamlanır.",
            )}
          </p>
          <footer className="gc-form-actions">
            <button
              className="primary-button"
              disabled={locked || !importText.trim()}
            >
              <FileUp size={15} />
              {text("Import metadata", "Bilgileri içe aktar")}
            </button>
          </footer>
        </form>
      </section>
      <section className="glass-panel gc-settings-card">
        <div className="gc-card-heading">
          <DatabaseBackup size={20} />
          <div>
            <h2>
              {text(
                "Full Hub database backups",
                "Tüm Hub veritabanı yedekleri",
              )}
            </h2>
            <p>
              {text(
                "All providers and settings in one SQLite snapshot",
                "Tüm sağlayıcılar ve ayarlar tek SQLite anlık görüntüsünde",
              )}
            </p>
          </div>
        </div>
        <p className="gc-hint">
          {text(
            "A database backup contains all Hub data, including existing protected provider credentials. Browser profiles are stored separately. Keep database backups private.",
            "Veritabanı yedeği, mevcut korumalı sağlayıcı kimlik bilgileri dahil tüm Hub verilerini içerir. Tarayıcı profilleri ayrı tutulur. Veritabanı yedeklerini özel tut.",
          )}
        </p>
        <div className="gc-toolbar">
          <button
            className="secondary-button"
            disabled={locked}
            onClick={() =>
              void run(
                () => googleApi.backup(),
                text(
                  "Full Hub database backup created.",
                  "Tüm Hub veritabanı yedeği oluşturuldu.",
                ),
              )
            }
          >
            <DatabaseBackup size={15} />
            {text("Create backup", "Yedek oluştur")}
          </button>
          <button
            className="secondary-button"
            disabled={locked}
            onClick={() => void chooseRestore()}
          >
            <RefreshCw size={15} />
            {text("Choose backup to restore…", "Geri yüklenecek yedeği seç…")}
          </button>
        </div>
        {!snapshot.backups.length ? (
          <p className="gc-muted">
            {text(
              "No local database backups yet.",
              "Henüz yerel veritabanı yedeği yok.",
            )}
          </p>
        ) : (
          <div className="gc-backup-list">
            {snapshot.backups.map((backup) => (
              <div key={backup.path}>
                <DatabaseBackup size={17} />
                <div>
                  <strong>{backup.name}</strong>
                  <small>
                    {timestamp(backup.createdAt, text)} ·{" "}
                    {(backup.size / 1024).toFixed(1)} KB
                  </small>
                  <small className="gc-backup-path">{backup.path}</small>
                </div>
                <button
                  className="card-action"
                  disabled={locked}
                  onClick={() => onRestore(backup.path)}
                >
                  {text("Restore…", "Geri yükle…")}
                </button>
              </div>
            ))}
          </div>
        )}
        <p className="gc-hint">
          <ShieldCheck size={14} />{" "}
          {text(
            "Restoring replaces all Hub data. A safety backup is created automatically before restore.",
            "Geri yükleme tüm Hub verilerini değiştirir. Öncesinde otomatik bir güvenlik yedeği oluşturulur.",
          )}
        </p>
      </section>
      <section className="glass-panel gc-settings-card">
        <h2>{text("Keyboard shortcuts", "Klavye kısayolları")}</h2>
        <div className="gc-shortcuts">
          <span>
            <kbd>Ctrl / ⌘ K</kbd>{" "}
            {text("Search Google accounts", "Google hesaplarında ara")}
          </span>
          <span>
            <kbd>/</kbd> {text("Search accounts", "Hesaplarda ara")}
          </span>
          <span>
            <kbd>Ctrl / ⌘ N</kbd>{" "}
            {text("New Google account", "Yeni Google hesabı")}
          </span>
          <span>
            <kbd>Esc</kbd> {text("Close dialog", "Pencereyi kapat")}
          </span>
        </div>
      </section>
    </div>
  );
}
