# Changelog

All public releases are listed here. Version numbers follow the app's package,
Rust crate, and Tauri bundle versions together.

## [Unreleased] - macOS port (`macos` branch)

- Bring the `macos` branch up to 0.1.24 and port the Windows-only parts:
  data, log and Claude/Antigravity paths under `~/Library`; app detection and
  launching through `.app` bundles; command-line tools found in Homebrew, npm
  and `~/.local/bin` folders.
- Encrypt Antigravity pool tokens with ChaCha20-Poly1305 under a key kept in
  the login Keychain, and switch Antigravity accounts through its Keychain item.
- Native macOS window buttons, menu-bar hiding with Dock reopen, macOS wording
  in the interface, and `.app`/`.dmg` bundles built on macOS in CI.
- macOS versions of the 0.1.24 Windows-only parts: Antigravity close and
  force-close through `pkill`, Copilot sign-in from `~/.config/github-copilot`,
  Cursor session from `~/Library/Application Support/Cursor`.

## [0.1.24] - 2026-10-12

- Group provider cards and saved accounts by provider, Google first, then
  Anthropic, ChatGPT, OpenCode and the rest. They were listed in mixed order.
- New usage sources: **Gemini CLI** (per-model quota with the CLI's own Google
  sign-in), **GitHub Copilot** (chat, completion and premium request quotas with
  the editor plugin's sign-in), **Cursor** (billing-cycle usage with Cursor's own
  session), and **DeepSeek**, **Kimi / Moonshot** and **Grok / xAI** with an API
  key pasted on the Integrations page. DeepSeek and Kimi show the account
  balance; xAI reports no balance for a regular key, so Grok shows only whether
  the key works.
- The proxy now rotates pool accounts for its own requests without switching
  the account open in the IDE. Before, a 429 or 403 on a proxy request also
  switched the IDE and restarted its language server.
- The proxy shows an account-risk notice once and answers 403 until it is
  accepted. Existing installs are treated as having accepted it.
- Proxy port and upstream timeout are configurable (port changes apply after a
  restart).
- Housekeeping requests from coding tools (conversation titles, next-prompt
  suggestions) go to a Flash model whatever model they asked for; can be turned
  off.
- Image generation honours `size`, `quality`, `imageSize`, `n` (1-10),
  `response_format: "url"` and model suffixes such as `-16-9-4k`.
- Model rules accept regular expressions (`re:^gpt-4`).
- New switch strategy "Best plan first": Ultra accounts before Pro before Free.
  The pool shows each account's plan.
- Per-account HTTP / SOCKS5 proxy for a pool account's Google traffic. A broken
  proxy fails the request; it never falls back to a direct connection.
- One-click connect and disconnect for Claude Code and OpenCode, which edits
  the tool's own config file and keeps a `.hub-backup` copy.
- Close and force-close buttons for the Antigravity IDE; sort the pool by Gemini
  or Claude/GPT quota; cURL and Python request examples.

## [0.1.23] - 2026-10-12

- Proxy key check now fails closed: a request is refused when settings cannot
  be read or no key is stored, and keys are compared in constant time.
- A pool account deleted while its quota was being refreshed, or during a
  switch, is no longer written back together with its tokens.

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
