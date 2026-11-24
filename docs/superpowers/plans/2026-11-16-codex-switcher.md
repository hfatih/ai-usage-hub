# Codex Switcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans. Execute inline in the existing clean checkout; user approved implementation, no further design approval needed.

**Goal:** Add saved Codex account login and verified manual switching to AI Usage Hub.
**Architecture:** The installed Codex 0.167.0-alpha.3.2 exposes accountSession/list and accountSession/switch. Use native credential storage and token renewal instead of duplicating tokens in Hub. Retain only hidden-session identifiers in SQLite. This implements the approved behavior using a smaller native mechanism; no auth.json copying, extra CODEX_HOME, token export, or forced desktop restart.
**Tech Stack:** Existing Rust/Tauri, React, SQLite, Vitest. No new dependencies.
**Spec:** ../specs/2026-11-16-codex-switcher.md, approved by user; native-storage amendment documented above.

## Global Constraints
- Product code stays in the existing external AI Usage Hub checkout.
- Never print or return authentication tokens; preserve existing Codex credentials and work.
- Native APIs must be supported; report upgrade guidance for old Codex versions.
- Report CLI/shared session verification accurately; desktop reload remains a separate condition.

## Review Focus
- Two workspaces for one login: switch must validate session and workspace IDs, not email alone.
- Cancelled/failed OAuth: retain the previous native session and release the owned callback process.
- Timeout or mismatched switch: restore the previous session or explicitly report rollback failure.
- Unsupported CLI: show actionable error and keep other providers working.
- Hidden Hub accounts: hide metadata only; clearly explain Codex still owns their saved sign-in.

### Task 1: Native client and account operations
Files: src-tauri/src/codex_client.rs, codex_accounts.rs, providers/openai.rs, database.rs, lib.rs.
- [ ] Write failing tests for workspace selection, unsupported protocol, switch validation, login completion filtering, failed/cancelled login handling, and hidden-session storage.
- [ ] Observe failures with cargo test --locked --lib.
- [ ] Implement owned child process cleanup, bounded RPC, validated browser URL, serialized operations, native login start/poll/cancel and verified switch/rollback.
- [ ] Pass focused Rust tests and existing suite.

### Task 2: Account UI and IPC
Files: src/pages/accounts/CodexAccounts.tsx, src/pages/AccountsPage.tsx, src/lib/ipc.ts, src/types.ts, src/i18n/tr.ts, src/styles.css, src-tauri/src/commands.rs.
- [ ] Add meaningful Vitest tests for workspace selection, display-only usage matching and login state handling.
- [ ] Observe failure; implement Codex section using existing card/buttons/materials, browser sign-in link, cancel, reload, workspace switch, local hide confirmation.
- [ ] Run npm test and npm run build, with existing npm binary.

### Task 3: Package and review
Files: docs/superpowers/plans/2026-11-16-codex-switcher.md, CHANGELOG.md, knowledge/history/engine_features.md, version files.
- [ ] Bump to 0.1.27, document native storage and desktop-reopen limitation.
- [ ] Review with a fresh code reviewer; fix material findings with regression checks.
- [ ] Run cargo check/test, frontend tests/build, build Windows installer with profile path remapping, compute checksum.
- [ ] Safely verify list integration on real installed Codex without changing the user's active account. Report real A/B acceptance pending a second account.
- [ ] Update source-backed memory and deliver artifact. Publishing is outside this requested local feature implementation; do not push without instruction.
