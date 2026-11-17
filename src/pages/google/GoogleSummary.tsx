import { useEffect, useState } from "react";
import { ArrowRight, Cloud } from "lucide-react";
import { googleApi, type GoogleSnapshot } from "../../lib/googleControl";
import { accountAttention, useGoogleText } from "./shared";

export function GoogleSummary({ onOpen }: { onOpen: () => void }) {
  const text = useGoogleText();
  const [snapshot, setSnapshot] = useState<GoogleSnapshot | null>(null);
  useEffect(() => {
    let active = true;
    googleApi
      .snapshot()
      .then((value) => {
        if (active) setSnapshot(value);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);
  if (!snapshot) return null;
  const accounts = snapshot.accounts.filter((a) => !a.archived);
  return (
    <section className="glass-panel gc-dashboard-summary">
      <span className="summary-icon">
        <Cloud size={19} />
      </span>
      <div>
        <strong>
          {text("Google control center", "Google kontrol merkezi")}
        </strong>
        <small>
          {accounts.length} {text("accounts", "hesap")} ·{" "}
          {
            accounts.filter((a) =>
              accountAttention(a, snapshot.preferences.expirationWarningDays),
            ).length
          }{" "}
          {text("need attention", "ilgi gerekiyor")} ·{" "}
          {snapshot.projects.filter((p) => p.status === "active").length}{" "}
          {text("active projects", "etkin proje")}
        </small>
      </div>
      <button className="text-button" onClick={onOpen}>
        {text("Open control center", "Kontrol merkezini aç")}
        <ArrowRight size={15} />
      </button>
    </section>
  );
}
