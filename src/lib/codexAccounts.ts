import type { AccountSummary, CodexSession } from "../types";

export function codexWorkspace(session: CodexSession, selected?: string): string {
  return [selected, session.selectedWorkspaceAccountId, session.workspaces[0]?.accountId]
    .find((id) => session.workspaces.some((workspace) => workspace.accountId === id)) ?? "";
}

export function codexSnapshot(session: CodexSession, workspace: string, accounts: AccountSummary[]): AccountSummary | undefined {
  return accounts.find((account) => account.providerId === "openai" && account.id === `openai:${workspace}`
    && session.email != null && account.email.toLowerCase() === session.email.toLowerCase());
}
