# Feature history

- 2026-10-12: Pool tokens encrypted at rest with DPAPI (`src-tauri/src/secret.rs`, `dpapi:` prefix, legacy rows migrated); proxy API key with regenerate and on/off switch; user model rules (`proxyModelMap`, prefix `*`); `/v1/images/generations`; Windows toast for new usage alerts via `tauri-plugin-notification`; multi-select delete in the account pool (v0.1.22).

- 2026-10-12: Live proxy model catalog (`get_proxy_models`, cached 10 minutes); catalog ids pass through `resolve_upstream_model` unchanged. Weekly reset times on pool quota chips (v0.1.21).

- 2026-10-12: Antigravity prompt and flow credit balances (`availablePromptCredits`, `availableFlowCredits` against their monthly allowance) are shown as usage rows (v0.1.19).

- 2026-09-24: Branded the Windows NSIS installer for v0.1.11 with the AI Usage Hub star icon and matching pink/burgundy header and sidebar artwork. The installer and uninstaller use the same icon.
- 2026-09-24: Scatter more stars across entire card backgrounds, with three fresh random layouts generated each time the UI starts (v0.1.12).
- 2026-09-24: Replace the dense tiny card star field with a few larger, static pink ✦ glyphs based on the supplied Starlit star reference (v0.1.13).
- 2026-09-24: Prepare v0.1.15 for a public MIT-licensed GitHub release with bilingual setup/privacy docs, contribution guidance, CI, and versioned installer releases.
- 2026-09-24: Default new v0.1.16 installs and the NSIS installer to English, keep Turkish selectable, and establish a separate macOS development branch.
