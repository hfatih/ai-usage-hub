# Bug fix history

- 2026-10-17: Antigravity account switch never completed. (1) Antigravity 2.24 reads its sign-in from `~/.gemini/jetski-standalone-oauth-token`, not Windows Credential Manager, so the restarted language server came back as the old account; the credential read/write functions now use that file first. (2) Credential Manager refused the payload with "not enough memory" (os error 8); that write is now best-effort when the token file was written, and drops `id_token` above the 2560-byte blob limit. (3) The switch and auto-switch activity events passed a pool id as `account_id`, which references `accounts`, so `PRAGMA foreign_keys` rejected them and the command returned an error after switching; they now pass `None`. Step timings are logged as `switch: …`.

- 2026-10-12: Proxy authentication fell open when the settings read failed or the stored key was empty; it now returns 503/401 and uses a constant-time comparison (v0.1.23).
- 2026-10-12: Refresh and switch paths saved an account with an upsert after an unlocked network call, re-creating an account (and its tokens) deleted in between; they now use `update_antigravity_account`, which only writes existing rows (v0.1.23).

- 2026-10-12: Security page hard-coded `stored_secret_count: 0` and said no provider secrets were stored while the Antigravity pool keeps OAuth tokens in SQLite; the count is now queried and the page describes the tokens, the keyless 127.0.0.1 proxy and exports (v0.1.21).
- 2026-10-12: Proxy test panel and `/v1/models` listed a fixed set of outdated model names plus a non-existent `imagen-3`; the list now comes from `fetchAvailableModels` (v0.1.21).

- 2026-10-12: Accounts and API Proxy pages used off-theme blue/green colours and unstyled inputs (`.search-input` and `select` had no rule); header buttons wrapped their labels; the shell tabs reused the 30px icon button class. Themed the controls, made the action row wrap, added `.segment-btn` for text tabs (v0.1.20).

- 2026-10-12: Google One AI credits showed `minimumCreditAmountForUsage` (50) as the balance; the balance now comes from the language server when it reports one, otherwise from Antigravity's saved `modelCredits` state (v0.1.19).
- 2026-10-12: Removed the fabricated "Imagen 3 & Multimodal" usage row (a copy of the Gemini 5-hour percentage) and the hard-coded "Antigravity Pro" plan label (v0.1.19).
- 2026-10-12: Switching to an account auto-added from the live session wrote an empty token to Windows Credential Manager; such accounts are now refused, and a switch is verified against the restarted language server and rolled back if the IDE keeps the old account (v0.1.19).
- 2026-10-12: `rate_limited` accounts stayed benched whenever Google's quota endpoint failed; the flag now clears after the known reset time, or five hours after last use (v0.1.19).
- 2026-10-12: Antigravity network calls ran while holding the database mutex and auto-switch forced a language-server refresh every monitor tick; both now run outside the lock and reuse the 60-second provider refresh (v0.1.19).

- 2026-09-24: Refresh the Windows shell icon cache after installation so desktop and Start menu shortcuts pick up the current embedded star icon.
- 2026-09-24: Keep the burgundy account card surface during hover; the earlier hover background shorthand removed its card background and exposed the purple page underneath.
- 2026-09-24: Restore the small pink star shapes and generate a separate unscaled star field for each card's actual size, so long Activity and Recent Alerts panels fill naturally without stretching the stars (v0.1.14).
- 2026-09-24: Keep integration app rows at the bottom of grid-stretched cards so shorter providers do not leave empty space below their app section (v0.1.15).
- 2026-09-24: Make account cards flex columns and push their footer to the bottom of the shared grid row, leaving the empty middle above the actions (v0.1.17).
- 2026-10-12: A 429 or 403 on a proxy request called the IDE auto-switch, so using the proxy could change the account open in Antigravity and restart its language server. The proxy now only flags the account and picks the next pool account for its own requests (v0.1.24).
