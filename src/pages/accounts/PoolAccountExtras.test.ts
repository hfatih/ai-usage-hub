import { expect, it } from "vitest";
import type { AntigravityAccount } from "../../types";
import { sortPoolAccounts, tierLabel } from "./PoolAccountExtras";

function account(email: string, gemini5h?: number, claude5h?: number): AntigravityAccount {
  return { id: email, email, displayName: null, accessToken: "", refreshToken: "", idToken: null, projectId: null, expiryTimestamp: 0, status: "healthy", isCurrent: false, lastUsed: 0, quotaRemainingPercent: null, quotaDetails: gemini5h == null ? null : { gemini5h, claude5h } };
}

it("sorts the pool by a model group's 5-hour window with unknown accounts last", () => {
  const pool = [account("unknown"), account("low", 10, 90), account("high", 80, 20)];
  expect(sortPoolAccounts(pool, "gemini").map((item) => item.email)).toEqual(["high", "low", "unknown"]);
  expect(sortPoolAccounts(pool, "claude").map((item) => item.email)).toEqual(["low", "high", "unknown"]);
});

it("names Google tiers and leaves unknown ones as reported", () => {
  expect([tierLabel("g1-ultra-tier"), tierLabel("g1-pro-tier"), tierLabel("free-tier"), tierLabel("legacy-tier"), tierLabel(null)]).toEqual(["Ultra", "Pro", "Free", "legacy-tier", null]);
});
