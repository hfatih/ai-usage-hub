import { useState } from "react";
import { Activity as ActivityIcon } from "lucide-react";
import type { GoogleSnapshot } from "../../lib/googleControl";
import { accountName, Empty, timestamp, useGoogleText } from "./shared";

export function Activity({
  snapshot,
  compact = false,
}: {
  snapshot: GoogleSnapshot;
  compact?: boolean;
}) {
  const text = useGoogleText();
  const [query, setQuery] = useState("");
  const [accountId, setAccountId] = useState("");
  const [kind, setKind] = useState("");
  const [projectId, setProjectId] = useState("");
  const events = snapshot.activity
    .filter(
      (a) =>
        (!accountId || a.accountId === accountId) &&
        (!kind || a.kind === kind) &&
        (!projectId || a.projectId === projectId) &&
        [a.title, a.detail]
          .join(" ")
          .toLowerCase()
          .includes(query.toLowerCase()),
    )
    .sort((a, b) => b.createdAt - a.createdAt)
    .slice(0, compact ? 6 : 300);
  return (
    <>
      {!compact && (
        <div className="gc-filters">
          <input
            className="gc-activity-search"
            placeholder={text("Search activity…", "Etkinlik ara…")}
            aria-label={text("Search activity", "Etkinliklerde ara")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <select
            aria-label={text("Activity account", "Etkinlik hesabı")}
            value={accountId}
            onChange={(e) => setAccountId(e.target.value)}
          >
            <option value="">{text("All accounts", "Tüm hesaplar")}</option>
            {snapshot.accounts.map((a) => (
              <option key={a.id} value={a.id}>
                {accountName(a)}
              </option>
            ))}
          </select>
          <select
            aria-label={text("Activity project", "Etkinlik projesi")}
            value={projectId}
            onChange={(e) => setProjectId(e.target.value)}
          >
            <option value="">{text("All projects", "Tüm projeler")}</option>
            {snapshot.projects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          <select
            aria-label={text("Activity type", "Etkinlik türü")}
            value={kind}
            onChange={(e) => setKind(e.target.value)}
          >
            <option value="">{text("All types", "Tüm türler")}</option>
            {[...new Set(snapshot.activity.map((a) => a.kind))]
              .sort()
              .map((k) => (
                <option key={k}>{k}</option>
              ))}
          </select>
        </div>
      )}
      {!events.length ? (
        <Empty>
          {text(
            "Activity will appear after account, service and project actions.",
            "Hesap, hizmet ve proje işlemlerinden sonra etkinlikler burada görünür.",
          )}
        </Empty>
      ) : (
        <div className="glass-panel gc-activity-list">
          {events.map((event) => (
            <article key={event.id}>
              <span className="gc-event-icon">
                <ActivityIcon size={15} />
              </span>
              <div>
                <strong>{event.title}</strong>
                {event.detail && <p>{event.detail}</p>}
                <small>
                  {event.accountId &&
                    `${accountName(snapshot.accounts.find((a) => a.id === event.accountId))} · `}
                  {event.projectId &&
                    `${snapshot.projects.find((p) => p.id === event.projectId)?.name ?? event.projectId} · `}
                  {event.kind}
                </small>
              </div>
              <time dateTime={new Date(event.createdAt * 1000).toISOString()}>
                {timestamp(event.createdAt, text)}
              </time>
            </article>
          ))}
        </div>
      )}
    </>
  );
}
