import type { UsageValue } from "../types";
import { currentLanguage, translate } from "../i18n";

export function formatDuration(seconds: number): string {
  const language = currentLanguage();
  const safe = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const secs = safe % 60;
  if (hours > 0) return `${hours}${language === "tr" ? " sa" : "h"} ${minutes.toString().padStart(2, "0")}${language === "tr" ? " dk" : "m"}`;
  if (minutes > 0) return `${minutes}${language === "tr" ? " dk" : "m"} ${secs.toString().padStart(2, "0")}${language === "tr" ? " sn" : "s"}`;
  return `${secs}${language === "tr" ? " sn" : "s"}`;
}

export function formatClock(seconds: number): string {
  const safe = Math.max(0, Math.floor(seconds));
  return [Math.floor(safe / 3600), Math.floor((safe % 3600) / 60), safe % 60]
    .map((part) => part.toString().padStart(2, "0"))
    .join(":");
}

export function relativeTime(epochSeconds: number | null): string {
  if (!epochSeconds) return translate("Never");
  const diff = Math.max(0, Math.floor(Date.now() / 1000) - epochSeconds);
  if (diff < 10) return translate("Just now");
  if (diff < 60) return translate("{count}s ago", currentLanguage(), { count: diff });
  if (diff < 3600) return translate("{count}m ago", currentLanguage(), { count: Math.floor(diff / 60) });
  if (diff < 86400) return translate("{count}h ago", currentLanguage(), { count: Math.floor(diff / 3600) });
  return new Intl.DateTimeFormat(currentLanguage() === "tr" ? "tr-TR" : "en-US", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(epochSeconds * 1000);
}

export function sourceLabel(source: string): string {
  if (source === "estimated") return translate("Estimated");
  if (source === "locally_calculated") return translate("Calculated");
  if (source === "unavailable") return translate("Unavailable");
  if (source.startsWith("official")) return translate("Official");
  return translate("Local");
}

export function resetTime(epochSeconds: number | null): string {
  if (!epochSeconds) return translate("Reset time unavailable");
  const absolute = formatDateTime(epochSeconds);
  const seconds = epochSeconds - Math.floor(Date.now() / 1000);
  if (seconds <= 0) return translate("Renewed · {date}", currentLanguage(), { date: absolute });
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.max(1, Math.floor((seconds % 3600) / 60));
  const tr = currentLanguage() === "tr";
  const relative = days > 0 ? `${days}${tr ? " gün" : "d"} ${hours}${tr ? " sa" : "h"}` : hours > 0 ? `${hours}${tr ? " sa" : "h"} ${minutes}${tr ? " dk" : "m"}` : `${minutes}${tr ? " dk" : "m"}`;
  return translate("Resets in {duration} · {date}", currentLanguage(), { duration: relative, date: absolute });
}

export function formatDateTime(epochSeconds: number): string {
  return new Intl.DateTimeFormat(currentLanguage() === "tr" ? "tr-TR" : "en-US", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(epochSeconds * 1000);
}

export function isPercentageUsage(usage: UsageValue): boolean {
  return usage.unit.includes("%");
}

export function formatUsageValue(usage: UsageValue): string {
  if (usage.value == null) return "—";
  let val = usage.value;
  // If reset time has passed, quota is renewed back to 100%
  if (usage.resetAt && Math.floor(Date.now() / 1000) >= usage.resetAt && isPercentageUsage(usage) && val <= 0) {
    val = 100;
  }
  if (isPercentageUsage(usage)) {
    if (val > 0 && val < 1) return "<1%";
    return `${Math.round(val)}%`;
  }
  if (usage.unit.toUpperCase() === "USD") {
    return new Intl.NumberFormat(currentLanguage() === "tr" ? "tr-TR" : "en-US", { style: "currency", currency: "USD", maximumFractionDigits: 4 }).format(usage.value);
  }
  if (usage.unit === "tokens") return `${Math.round(usage.value).toLocaleString(currentLanguage() === "tr" ? "tr-TR" : "en-US")} ${translate("tokens")}`;
  return `${Number.isInteger(usage.value) ? usage.value : usage.value.toFixed(2)}${usage.unit ? ` ${translate(usage.unit)}` : ""}`;
}

export function usageProgress(usage: UsageValue): number | null {
  if (usage.value == null || usage.maxValue == null || usage.maxValue <= 0) return null;
  let val = usage.value;
  if (usage.resetAt && Math.floor(Date.now() / 1000) >= usage.resetAt && isPercentageUsage(usage) && val <= 0) {
    val = 100;
  }
  return Math.min(100, Math.max(0, val / usage.maxValue * 100));
}

export function usageTiming(usage: UsageValue): string {
  if (usage.resetAt) return usage.source === "locally_calculated" ? `${translate("Estimated")} · ${resetTime(usage.resetAt)}` : resetTime(usage.resetAt);
  if (usage.source === "local_logs") return translate("This device · last 24 hours");
  if (usage.unit === "models") return translate("Live model catalog");
  if (usage.unit === "RPM") return translate("Per-minute ceiling");
  if (usage.source === "local_application" || usage.source === "locally_calculated") return translate("Local tracked total");
  return translate("No reset reported");
}

export function shortResetTime(epochSeconds: number | null): string {
  if (!epochSeconds) return "";
  const seconds = epochSeconds - Math.floor(Date.now() / 1000);
  if (seconds <= 0) return currentLanguage() === "tr" ? "Yenilendi" : "Reset";
  const tr = currentLanguage() === "tr";
  const days = Math.floor(seconds / 86400);
  if (days > 0) return `${days}${tr ? "g" : "d"} ${Math.floor((seconds % 86400) / 3600)}${tr ? "sa" : "h"}`;
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.max(1, Math.floor((seconds % 3600) / 60));
  if (hours > 0) return `${hours}${tr ? "sa" : "h"} ${minutes}${tr ? "dk" : "m"}`;
  return `${minutes}${tr ? "dk" : "m"}`;
}
