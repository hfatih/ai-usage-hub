import { afterEach, describe, expect, it, vi } from "vitest";
import type { GoogleAccount, GoogleProject } from "../../lib/googleControl";
import {
  accountAttention,
  accountInput,
  expirationDays,
  projectInput,
} from "./shared";

const account: GoogleAccount = {
  id: "google-test",
  email: "test@example.com",
  name: "Research",
  color: "#dba8c2",
  subscription: "AI Pro",
  activationDate: "2026-11-01",
  expirationDate: "2026-11-18",
  enabled: true,
  archived: false,
  favorite: true,
  group: "Research",
  tags: ["science"],
  notes: "Manual metadata",
  antigravityId: "native-test",
  lastUsed: 1,
  createdAt: 2,
  quotaRemaining: 75,
  quotaUpdatedAt: 3,
  quotaDetails: '{"model":"reported"}',
};

afterEach(() => vi.useRealTimers());

describe("subscription calendar dates", () => {
  it("counts calendar days regardless of time of day", () => {
    expect(expirationDays(account, new Date(2026, 10, 17, 23, 59))).toBe(1);
    expect(expirationDays(account, new Date(2026, 10, 18, 0, 1))).toBe(0);
    expect(expirationDays(account, new Date(2026, 10, 19, 0, 1))).toBe(-1);
  });
  it("handles year boundaries and absent expiration without inventing dates", () => {
    expect(
      expirationDays(
        { ...account, expirationDate: "2027-01-01" },
        new Date(2026, 11, 31, 18),
      ),
    ).toBe(1);
    expect(expirationDays({ ...account, expirationDate: null })).toBeNull();
  });
  it("includes warning boundary and expired accounts, excluding archived entries", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 10, 17, 12));
    expect(accountAttention(account, 1)).toBe(true);
    expect(accountAttention(account, 0)).toBe(false);
    expect(
      accountAttention({ ...account, expirationDate: "2026-11-01" }, 0),
    ).toBe(true);
    expect(accountAttention({ ...account, archived: true }, 7)).toBe(false);
    expect(
      accountAttention({ ...account, enabled: false, expirationDate: null }, 0),
    ).toBe(true);
  });
});

describe("strict Google IPC input boundaries", () => {
  it("excludes native fields and timestamps from account saves", () => {
    const input = accountInput(account);
    expect(Object.keys(input).sort()).toEqual(
      [
        "id",
        "email",
        "name",
        "color",
        "subscription",
        "activationDate",
        "expirationDate",
        "enabled",
        "archived",
        "favorite",
        "group",
        "tags",
        "notes",
      ].sort(),
    );
    expect(input.email).toBe(account.email);
    expect(input.tags).toEqual(account.tags);
    input.tags.push("edit");
    expect(account.tags).toEqual(["science"]);
  });
  it("excludes read-only project fields while preserving roles and service assignments", () => {
    const project: GoogleProject = {
      id: "project-test",
      name: "Science",
      description: "Research",
      notes: "Project note",
      status: "active",
      assignments: [
        {
          accountId: account.id,
          role: "Analysis",
          services: ["notebooklm", "gemini"],
        },
      ],
      createdAt: 1,
      updatedAt: 2,
    };
    const input = projectInput(project);
    expect(Object.keys(input).sort()).toEqual(
      ["id", "name", "description", "notes", "status", "assignments"].sort(),
    );
    expect(input.assignments).toEqual(project.assignments);
    input.assignments[0].services.push("flow");
    expect(project.assignments[0].services).toEqual(["notebooklm", "gemini"]);
  });
});
