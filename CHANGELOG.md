# Changelog

All public releases are listed here. Version numbers follow the app's package,
Rust crate, and Tauri bundle versions together.

## [0.1.22] - 2026-10-12

- Encrypt Antigravity sign-in tokens in the local database with Windows data
  protection (DPAPI). Tokens saved by earlier versions are converted on first start.
- Protect the local API proxy with an API key. A key is generated on first
  start and shown on the API Proxy page; **tools configured with the old
  placeholder key must be updated**, or the check can be switched off there.
- Add model rules: send any model name a tool asks for to a model you choose.
- Add an OpenAI-style image endpoint (`/v1/images/generations`) backed by the
  catalog's image model.
- Show a Windows notification for new usage alerts.
- Select several pool accounts and delete them together.

## [0.1.21] - 2026-10-12

- Read the proxy's model list live from Google instead of a fixed list of old
  model names; real model ids now pass through unchanged, and the test panel
  and `/v1/models` show them.
- Show the weekly reset time on the Gemini and Claude weekly quota chips.
- Security page now reports the Antigravity sign-in tokens kept in the local
  database, the keyless local proxy and what an account export contains. It
  previously claimed no secrets were stored.
- Keep one Refresh button in the top bar; remove the duplicates on the
  Dashboard, Accounts, Apps and Integrations pages.
- More stars on cards, white "verified session" badge, copy buttons kept inside
  their snippet, and "İzlemeyi duraklat" word order in Turkish.

## [0.1.20] - 2026-10-12

- Restyle the live Antigravity session card, search boxes and dropdowns to the
  app's pink and burgundy theme; they were blue, green and browser-default grey.
- Keep header button labels on one line and wrap the button row instead.
- Fix the Bash / PowerShell tabs on the API Proxy page, whose labels were
  squeezed into icon-sized buttons, and inset the snippet blocks from the card edge.

## [0.1.19] - 2026-10-12

- Show the real Google One AI credit balance instead of the 50-credit usage
  threshold, and add Antigravity's prompt and flow credit balances.
- Remove the "Imagen 3 & Multimodal" row: it was a copy of the Gemini 5-hour
  percentage, not a quota Antigravity reports.
- Refuse to switch to an account that has no saved sign-in token, verify the
  account the language server reports after a switch, and restore the previous
  credential when the switch did not take effect.
- Stop auto-switch from bouncing between two low accounts and from forcing a
  quota refresh every few seconds; it now reuses the regular provider refresh.
- Clear a stale rate-limited flag once the quota window has reset.
- Keep the app responsive while Google is slow: account network calls no longer
  hold the database lock, and opening the account list no longer waits on them.

## [0.1.18] - 2026-10-12

- Authoritative 3-tier live session detection for Google Antigravity accounts.
- Zero-restart instant hot-switching and automatic 429 rate-limit rotation.
- High-performance local AI API Proxy Gateway on port 8046 (OpenAI & Anthropic dual-protocol relay).
- Full support for Claude 3.7 Sonnet reasoning budget and silent background downgrading.
- Rolling 5-hour quota sliding window calculations.
- Refined card starlight visual density with organic 4-pointed sparkles and soft stardust dots.
- Complete bilingual documentation suite (TR | EN) and SVG UI tour gallery.

## [0.1.17] - 2026-09-24

- Anchor account card actions to the bottom of each grid-stretched card, leaving
  any extra space between usage details and the footer.

## [0.1.16] - 2026-09-24

- Make English the default language for new installations and the Windows
  installer. Turkish remains available in Settings.
- Preserve the language selected by existing users during upgrades.
- Reserve a separate `macos` development branch for the future Mac port.

## [0.1.15] - 2026-09-24

Initial public release of AI Usage Hub for Windows.

- Local application monitoring, session history, launch shortcuts, and alerts.
- Source-aware usage cards for Codex, Claude, Antigravity, and OpenCode.
- Provider and account separation, with unavailable data clearly labeled.
- Turkish and English interface, pink star theme, custom title bar, and NSIS installer.
- Local SQLite storage without AI Usage Hub telemetry.

The Windows installer is currently unsigned. Automatic binary updates are not
enabled; install a newer release over the existing installation to update.
