import {
  AlertTriangle,
  ArrowRight,
  Clock3,
  FolderOpen,
  Star,
  Users,
} from "lucide-react";
import type { GoogleSnapshot } from "../../lib/googleControl";
import { Activity } from "./Activity";
import { Services } from "./Services";
import {
  accountAttention,
  accountName,
  Empty,
  Expiry,
  expirationDays,
  timestamp,
  useGoogleText,
  type Run,
  type Tab,
} from "./shared";

export function Overview({
  snapshot,
  busy,
  run,
  onTab,
}: {
  snapshot: GoogleSnapshot;
  busy: boolean;
  run: Run;
  onTab: (tab: Tab) => void;
}) {
  const text = useGoogleText();
  const registered = snapshot.accounts.filter((a) => !a.archived);
  const expiring = registered.filter((a) => {
    const days = expirationDays(a);
    return (
      days !== null &&
      days >= 0 &&
      days <= snapshot.preferences.expirationWarningDays
    );
  });
  const attention = registered.filter(
    (a) =>
      accountAttention(a, snapshot.preferences.expirationWarningDays) ||
      snapshot.services.some((s) =>
        s.statuses.some(
          (status) =>
            status.accountId === a.id &&
            (status.status === "login_required" ||
              status.status === "unavailable"),
        ),
      ),
  );
  const today = new Date();
  const localToday = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`;
  const currentPro = registered.filter(
    (a) =>
      a.enabled &&
      /\bpro\b/i.test(a.subscription) &&
      a.expirationDate !== null &&
      a.expirationDate >= localToday &&
      (!a.activationDate || a.activationDate <= localToday),
  );
  const recent = [...registered]
    .filter((a) => a.lastUsed)
    .sort((a, b) => (b.lastUsed ?? 0) - (a.lastUsed ?? 0))
    .slice(0, 5);
  const projects = snapshot.projects
    .filter((p) => p.status === "active")
    .slice(0, 4);
  const usage = [...snapshot.usage]
    .sort((a, b) => b.recordedAt - a.recordedAt)
    .filter(
      (r, index, all) =>
        all.findIndex(
          (x) =>
            x.accountId === r.accountId &&
            x.serviceId === r.serviceId &&
            x.metric === r.metric,
        ) === index,
    )
    .slice(0, 4);
  return (
    <>
      <div className="gc-overview-stats">
        {[
          {
            icon: Users,
            count: registered.length,
            label: text("Registered accounts", "Kayıtlı hesaplar"),
            sub: text("Unarchived entries", "Arşivlenmemiş kayıtlar"),
            tab: "accounts",
          },
          {
            icon: Star,
            count: currentPro.length,
            label: text(
              "Pro subscriptions in date",
              "Tarihi geçerli Pro abonelikleri",
            ),
            sub: text(
              "From your labels and dates",
              "Girdiğin etiket ve tarihlere göre",
            ),
            tab: "accounts",
          },
          {
            icon: AlertTriangle,
            count: attention.length,
            label: text("Need attention", "İlgi gerekiyor"),
            sub: text(
              "Status and expiration warnings",
              "Durum ve bitiş uyarıları",
            ),
            tab: "accounts",
          },
          {
            icon: Clock3,
            count: expiring.length,
            label: text("Expiring soon", "Yakında sona eriyor"),
            sub: text(
              `Within ${snapshot.preferences.expirationWarningDays} days`,
              `${snapshot.preferences.expirationWarningDays} gün içinde`,
            ),
            tab: "accounts",
          },
        ].map(({ icon: Icon, count, label, sub, tab }) => (
          <button
            className="glass-panel gc-stat"
            key={label}
            onClick={() => onTab(tab as Tab)}
          >
            <Icon size={18} />
            <strong>{count}</strong>
            <span>{label}</span>
            <small>{sub}</small>
          </button>
        ))}
      </div>
      {snapshot.preferences.notificationsEnabled && expiring.length > 0 && (
        <div className="gc-notice gc-notice--warning">
          <AlertTriangle size={18} />
          <div>
            <strong>
              {text(
                "Upcoming subscription expirations",
                "Yaklaşan abonelik bitişleri",
              )}
            </strong>
            <p>
              {expiring
                .slice(0, 4)
                .map((a) => `${accountName(a)} · ${a.expirationDate}`)
                .join("  /  ")}
            </p>
          </div>
          <button className="text-button" onClick={() => onTab("accounts")}>
            {text("Review", "İncele")}
            <ArrowRight size={14} />
          </button>
        </div>
      )}
      <div className="gc-section-heading">
        <div>
          <h2>{text("Service shortcuts", "Hizmet kısayolları")}</h2>
          <p>
            {text(
              "Choose the account before opening a service.",
              "Hizmeti açmadan önce hesabı seç.",
            )}
          </p>
        </div>
        <button className="text-button" onClick={() => onTab("services")}>
          {text("All services & status", "Tüm hizmetler ve durum")}
          <ArrowRight size={14} />
        </button>
      </div>
      <Services snapshot={snapshot} busy={busy} run={run} compact />
      <div className="gc-overview-columns">
        <section>
          <div className="gc-section-heading">
            <h2>{text("Recently used accounts", "Son kullanılan hesaplar")}</h2>
            <button
              className="text-button"
              onClick={() => onTab("accounts")}
              aria-label={text("View accounts", "Hesapları görüntüle")}
            >
              <ArrowRight size={15} />
            </button>
          </div>
          {!recent.length ? (
            <Empty>
              {text(
                "Open a service to record an account's last use.",
                "Hesabın son kullanımını kaydetmek için bir hizmet aç.",
              )}
            </Empty>
          ) : (
            <div className="glass-panel gc-recent-list">
              {recent.map((account) => (
                <div key={account.id}>
                  <span className="gc-avatar" style={{ color: account.color }}>
                    {accountName(account).slice(0, 2).toUpperCase()}
                  </span>
                  <div>
                    <strong>{accountName(account)}</strong>
                    <small>{account.email}</small>
                    <Expiry account={account} />
                  </div>
                  <time>{timestamp(account.lastUsed, text)}</time>
                </div>
              ))}
            </div>
          )}
        </section>
        <section>
          <div className="gc-section-heading">
            <h2>{text("Active projects", "Etkin projeler")}</h2>
            <button
              className="text-button"
              onClick={() => onTab("projects")}
              aria-label={text("View projects", "Projeleri görüntüle")}
            >
              <ArrowRight size={15} />
            </button>
          </div>
          {!projects.length ? (
            <Empty>
              {text(
                "Create a project to organize accounts by role.",
                "Hesapları rollerine göre düzenlemek için proje oluştur.",
              )}
            </Empty>
          ) : (
            <div className="glass-panel gc-recent-list">
              {projects.map((project) => (
                <button
                  key={project.id}
                  className="gc-project-shortcut"
                  onClick={() => onTab("projects")}
                >
                  <FolderOpen size={19} />
                  <div>
                    <strong>{project.name}</strong>
                    <small>
                      {project.assignments.length}{" "}
                      {text("assigned accounts", "atanmış hesap")}
                    </small>
                    <small>{project.description}</small>
                  </div>
                  <ArrowRight size={14} />
                </button>
              ))}
            </div>
          )}
        </section>
      </div>
      <div className="gc-section-heading">
        <div>
          <h2>
            {text("Latest measurements by account", "Hesaba göre son ölçümler")}
          </h2>
          <p>
            {text(
              "Separate records; accounts do not share credits or storage.",
              "Ayrı kayıtlar; hesaplar kredi veya depolama paylaşmaz.",
            )}
          </p>
        </div>
        <button className="text-button" onClick={() => onTab("usage")}>
          {text("Usage & quotas", "Kullanım ve kotalar")}
          <ArrowRight size={14} />
        </button>
      </div>
      {!usage.length ? (
        <Empty>
          {text(
            "No measurements recorded yet. Open Usage to inspect native quotas or enter manual measurements.",
            "Henüz ölçüm kaydı yok. Yerel kotaları incelemek veya manuel ölçüm girmek için Kullanım'ı aç.",
          )}
        </Empty>
      ) : (
        <div className="gc-measurement-grid">
          {usage.map((record) => (
            <div className="glass-panel gc-measurement" key={record.id}>
              <strong>
                {accountName(
                  snapshot.accounts.find((a) => a.id === record.accountId),
                )}
              </strong>
              <span>
                {snapshot.services.find((s) => s.id === record.serviceId)?.name}{" "}
                · {record.metric}
              </span>
              <b>
                {record.value.toLocaleString(text("en-US", "tr-TR"))}{" "}
                {record.unit}
              </b>
              <small>
                {record.source === "manual"
                  ? text("Manual", "Manuel")
                  : text("Native Antigravity", "Yerel Antigravity")}{" "}
                · {timestamp(record.recordedAt, text)}
              </small>
            </div>
          ))}
        </div>
      )}
      <div className="gc-section-heading">
        <h2>{text("Recent activity", "Son etkinlikler")}</h2>
        <button className="text-button" onClick={() => onTab("activity")}>
          {text("View activity", "Etkinlikleri görüntüle")}
          <ArrowRight size={14} />
        </button>
      </div>
      <Activity snapshot={snapshot} compact />
    </>
  );
}
