# Feature history

- 2026-10-12: Pool tokens encrypted at rest with DPAPI (`src-tauri/src/secret.rs`, `dpapi:` prefix, legacy rows migrated); proxy API key with regenerate and on/off switch; user model rules (`proxyModelMap`, prefix `*`); `/v1/images/generations`; Windows toast for new usage alerts via `tauri-plugin-notification`; multi-select delete in the account pool (v0.1.22).

- 2026-10-12: Live proxy model catalog (`get_proxy_models`, cached 10 minutes); catalog ids pass through `resolve_upstream_model` unchanged. Weekly reset times on pool quota chips (v0.1.21).

- 2026-10-12: Antigravity prompt and flow credit balances (`availablePromptCredits`, `availableFlowCredits` against their monthly allowance) are shown as usage rows (v0.1.19).

- 2026-09-24: Branded the Windows NSIS installer for v0.1.11 with the AI Usage Hub star icon and matching pink/burgundy header and sidebar artwork. The installer and uninstaller use the same icon.
- 2026-09-24: Scatter more stars across entire card backgrounds, with three fresh random layouts generated each time the UI starts (v0.1.12).
- 2026-09-24: Replace the dense tiny card star field with a few larger, static pink ✦ glyphs based on the supplied Starlit star reference (v0.1.13).
- 2026-09-24: Prepare v0.1.15 for a public MIT-licensed GitHub release with bilingual setup/privacy docs, contribution guidance, CI, and versioned installer releases.
- 2026-09-24: Default new v0.1.16 installs and the NSIS installer to English, keep Turkish selectable, and establish a separate macOS development branch.
- 2026-10-16: Account pool: small link icon next to "Sign in with Google" copies the OAuth URL and starts the port 8045 listener without opening a browser (`start_antigravity_oauth_login(open_browser)`), so the link can be opened in any browser or profile on this PC. Single instance via `tauri-plugin-single-instance`: a second launch exits and focuses the running window (`show_main`).
- 2026-10-12: Provider cards and saved accounts are grouped by provider in a fixed order with a heading per group; order lives in `src/lib/providers.ts` (v0.1.24).
- 2026-10-12: New provider sources (v0.1.24): Gemini CLI (`providers/gemini_cli.rs`, `retrieveUserQuota`), GitHub Copilot (`providers/copilot.rs`, `copilot_internal/user`), Cursor (`providers/cursor.rs`, `usage-summary`, untested against a live install), and key-based DeepSeek / Kimi / Grok (`providers/keyed.rs`; keys in the `provider_keys` table, DPAPI-encrypted).
- 2026-10-12: Proxy (v0.1.24): own account rotation without switching the IDE, account-risk gate, configurable port and timeout, background-task routing to Flash, image `size`/`quality`/`n`/suffix options, regex model rules, tier-priority strategy, per-account HTTP/SOCKS5 proxy. Pure routing decisions live in `src-tauri/src/routing/`.
- 2026-10-12: One-click Claude Code / OpenCode connection (`src-tauri/src/cli_config/`), Antigravity close / force-close, pool sorting by model group, cURL and Python examples (v0.1.24).
