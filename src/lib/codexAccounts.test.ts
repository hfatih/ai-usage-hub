import { describe, expect, it } from "vitest";
import { codexSnapshot, codexWorkspace } from "./codexAccounts";
import type { AccountSummary, CodexSession } from "../types";

const session: CodexSession = {
  sessionId: "login-a", email: "one@example.com", isActive: true, lastUsedAt: 1,
  selectedWorkspaceAccountId: "personal", workspaces: [{ accountId: "personal", name: "Personal" }, { accountId: "team", name: "Team" }],
};

describe("Codex saved accounts", () => {
  it("uses only a workspace belonging to this login", () => {
    expect(codexWorkspace(session, "team")).toBe("team");
    expect(codexWorkspace(session, "deleted")).toBe("personal");
    expect(codexWorkspace({ ...session, selectedWorkspaceAccountId: null })).toBe("personal");
    expect(codexWorkspace({ ...session, workspaces: [] })).toBe("");
  });

  it("does not attach another workspace or user's quota by matching only an email", () => {
    const snapshots = [
      { id: "openai:team", providerId: "openai", email: "one@example.com", plan: "Business" },
      { id: "openai:personal", providerId: "openai", email: "another@example.com", plan: "Pro" },
    ] as AccountSummary[];
    expect(codexSnapshot(session, "personal", snapshots)).toBeUndefined();
    expect(codexSnapshot(session, "team", snapshots)?.plan).toBe("Business");
  });
});
