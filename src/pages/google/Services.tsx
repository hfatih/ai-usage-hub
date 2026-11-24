import { useState } from "react";
import { useHub } from "../../stores/hub";
import { ArrowUpRight, ExternalLink, Star } from "lucide-react";
import {
  googleApi,
  type GoogleService,
  type GoogleServiceStatus,
  type GoogleSnapshot,
} from "../../lib/googleControl";
import {
  AccountSelect,
  Empty,
  timestamp,
  useGoogleText,
  type Run,
} from "./shared";

export function Services({
  snapshot,
  busy,
  run,
  compact = false,
}: {
  snapshot: GoogleSnapshot;
  busy: boolean;
  run: Run;
  compact?: boolean;
}) {
  const text = useGoogleText();
  const [favoritesOnly, setFavoritesOnly] = useState(false);
  const services = snapshot.services
    .filter((s) => !favoritesOnly || s.favorite)
    .sort((a, b) => Number(b.favorite) - Number(a.favorite))
    .slice(0, compact ? 4 : undefined);
  return (
    <>
      {!compact && (
        <div className="gc-toolbar">
          <p className="gc-hint">
            {text(
              "Each account opens in its own persistent browser profile. Complete Google sign-in there, then confirm the service status below.",
              "Her hesap kendine ait kalıcı tarayıcı profilinde açılır. Google girişini orada tamamla, ardından hizmet durumunu aşağıda doğrula.",
            )}
          </p>
          <label className="gc-check-label">
            <input
              type="checkbox"
              checked={favoritesOnly}
              onChange={(e) => setFavoritesOnly(e.target.checked)}
            />
            {text("Favorites only", "Yalnız favoriler")}
          </label>
        </div>
      )}
      {!services.length ? (
        <Empty>
          {text(
            "No favorite services yet. Uncheck the filter and star a service.",
            "Henüz favori hizmet yok. Filtreyi kaldırıp bir hizmeti yıldızla.",
          )}
        </Empty>
      ) : (
        <div
          className={`gc-services-grid${compact ? " gc-services-grid--compact" : ""}`}
        >
          {services.map((service) => (
            <ServiceCard
              key={service.id}
              service={service}
              snapshot={snapshot}
              busy={busy}
              run={run}
              compact={compact}
            />
          ))}
        </div>
      )}
      {!compact && (
        <p className="gc-hint">
          {text(
            "A subscription label does not verify entitlement to a service. Quotas and benefits belong to each account separately. Gemini CLI opens its official documentation.",
            "Abonelik etiketi hizmet kullanım hakkını doğrulamaz. Kota ve haklar her hesaba ayrı aittir. Gemini CLI resmî belgelerini açar.",
          )}
        </p>
      )}
    </>
  );
}
function ServiceCard({
  service,
  snapshot,
  busy,
  run,
  compact,
}: {
  service: GoogleService;
  snapshot: GoogleSnapshot;
  busy: boolean;
  run: Run;
  compact: boolean;
}) {
  const text = useGoogleText();
  const accounts = snapshot.accounts.filter((a) => a.enabled && !a.archived);
  const [chosen, setChosen] = useState(service.lastAccountId ?? "");
  const accountId = accounts.some((a) => a.id === chosen) ? chosen : "";
  const selectedAccount = accounts.find((account) => account.id === accountId);
  const status = service.statuses.find((s) => s.accountId === accountId);
  const labels: Record<GoogleServiceStatus, string> = {
    unknown: text("Unknown", "Bilinmiyor"),
    available: text("Available (I confirmed)", "Kullanılabilir (doğruladım)"),
    login_required: text("Sign-in required", "Giriş gerekiyor"),
    unavailable: text("Unavailable", "Kullanılamıyor"),
  };
  return (
    <article className="glass-panel gc-service-card">
      <div className="gc-card-heading">
        <span className="gc-service-orb">
          <ExternalLink size={18} />
        </span>
        <div>
          <h3>{service.name}</h3>
          <small>{new URL(service.url).hostname}</small>
        </div>
        <button
          className={`icon-button${service.favorite ? " gc-favorite" : ""}`}
          aria-label={text(
            `Favorite ${service.name}`,
            `${service.name} favorisi`,
          )}
          aria-pressed={service.favorite}
          disabled={busy}
          onClick={() =>
            void run(() =>
              googleApi.favoriteService(service.id, !service.favorite),
            )
          }
        >
          <Star size={15} fill={service.favorite ? "currentColor" : "none"} />
        </button>
      </div>
      <AccountSelect
        accounts={accounts}
        value={accountId}
        onChange={setChosen}
        disabled={busy}
        label={text(`${service.name} account`, `${service.name} hesabı`)}
      />
      {!compact && (
        <label className="gc-field gc-service-status">
          <span>
            {text(
              "Service status · user-confirmed",
              "Hizmet durumu · kullanıcı doğrulaması",
            )}
          </span>
          <select
            value={status?.status ?? "unknown"}
            disabled={busy || !accountId}
            onChange={(e) =>
              void run(
                () =>
                  googleApi.setServiceStatus(
                    accountId,
                    service.id,
                    e.target.value as GoogleServiceStatus,
                  ),
                text(
                  "Service status updated from your confirmation.",
                  "Hizmet durumu senin doğrulamanla güncellendi.",
                ),
              )
            }
          >
            {Object.entries(labels).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
          <small>
            {text("Last confirmed", "Son doğrulama")}:{" "}
            {timestamp(status?.updatedAt ?? null, text)}
          </small>
        </label>
      )}
      <button
        className="secondary-button gc-launch-button"
        disabled={busy || !accountId}
        onClick={() =>
          void run(
            () => googleApi.launchService(accountId, service.id),
            text(
              `${service.name} opened in the selected account's browser profile.`,
              `${service.name} seçilen hesabın tarayıcı profilinde açıldı.`,
            ),
          )
        }
      >
        {text("Open with selected account", "Seçilen hesapla aç")}
        <ArrowUpRight size={15} />
      </button>
      {!compact &&
        service.id === "antigravity" &&
        selectedAccount?.antigravityId && (
          <button
            className="card-action"
            disabled={busy}
            onClick={() =>
              void run(
                async () => {
                  await useHub
                    .getState()
                    .switchAntigravityAccount(selectedAccount.antigravityId!);
                  return googleApi.snapshot();
                },
                text(
                  "Antigravity IDE switched to the selected linked account.",
                  "Antigravity IDE seçilen bağlı hesaba geçirildi.",
                ),
              )
            }
          >
            {text(
              "Switch Antigravity IDE to this account",
              "Antigravity IDE bu hesaba geçsin",
            )}
          </button>
        )}
      {!accounts.length && (
        <small className="gc-warning">
          {text(
            "Add or enable an account to launch.",
            "Açmak için hesap ekle veya etkinleştir.",
          )}
        </small>
      )}
    </article>
  );
}
