# Changelog

All public releases are listed here. Version numbers follow the app's package,
Rust crate, and Tauri bundle versions together.

## [Unreleased] - macOS port (`macos` branch)

- Bring the `macos` branch up to 0.1.26 and port the Windows-only parts:
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


## [0.2.1] - 2026-11-17

- Add GitHub release checks in Settings with installed and published versions,
  a clear status when no update is available, and verified Windows installation.

## [0.2.0] - 2026-11-17

- Add a Google control center linked to existing Antigravity accounts, with
  account metadata, subscriptions, tags, groups, favorites and archive controls.
- Launch eight Google services in persistent, isolated Chrome or Edge profiles.
  Remember each service's last account and track user-confirmed availability.
- Organize projects with account roles and services, record clearly labeled
  manual usage, and show observed native Antigravity quota history.
- Add credential-free JSON/CSV metadata transfer, full SQLite backups and
  verified restoration, daily automatic backups and subscription notifications.
- Integrate navigation, dashboard summary, native account management links,
  keyboard search and the saved light/dark/system theme.
- Preserve renewed CLI credentials, keep removed accounts hidden across profile
  changes, reconcile failed switch responses, and verify Codex workspace identity.
- Respect disabled Google accounts in native Antigravity/proxy selection while
  retaining the real current account for automatic-switch trigger detection.

## [0.1.35] - 2026-11-17

- Apply account-card switching to the default Codex or Claude Code CLI login.
  Preserve the previous account in a profile, verify the CLI identity, and
  restore the previous login if verification fails.
- Show the actual active CLI account, consolidate duplicate accounts, and
  present unfinished sign-ins separately from connected account cards.
- Align account-pool search and primary actions with view and sorting controls
  on a separate row.

## [0.1.34] - 2026-11-16

- Remove and restore Codex and Claude Code cards in their Hub account pools.
  The provider's saved sign-in stays available to the CLI.

## [0.1.33] - 2026-11-16

- Show saved Codex and Claude Code profiles in Antigravity-style account
  pools with side-by-side cards, list view, search, sorting, and refresh.
- Select a profile or open its CLI from the card, with plan and saved quota
  details when available.

## [0.1.32] - 2026-11-16

- Add **Sign in with Google** and **Copy sign-in link** for Claude Code profiles.
  Continue with Google on Claude's official sign-in page. Claude Code receives
  its own callback, and Hub's local tab shows the verified result in that browser.
- Starting from a signed-in Claude profile creates a separate local profile.
  Copying the link leaves the browser closed until the link is opened.

## [0.1.31] - 2026-11-16

- Copy a local Codex sign-in page that can be opened in the browser of your
  choice. It shows the verified result in that browser, with no new browser
  launch from Hub after sign-in.
- Start additional sign-ins in a fresh profile even when a named profile is
  selected. List the saved profiles and their account emails together.
  Explain when a browser signs in with an email already used by another profile.

## [0.1.30] - 2026-11-16

- Keep Codex sign-in and copy-link available when the default profile is already
  signed in. Create a new profile for the additional account and retain the
  existing default login. Fall back to the native Windows clipboard when the
  WebView clipboard is unavailable.
- Wait for Claude Code sign-in and verify the selected profile automatically.
  Surface failed or cancelled sign-in and cap the wait.

## [0.1.29] - 2026-11-16

- Add separate local profiles for different Codex and Claude Code accounts.
  Add a profile, sign in to it once and launch the CLI in the selected project.
  Codex's native workspace switching remains available within each account.
- After verified Codex or Claude Code sign-in, open a separate browser tab
  with Hub's star-themed confirmation.

## [0.1.28] - 2026-11-16

- **Copy Codex sign-in link:** starts a managed browser sign-in without opening
  the default browser, then copies its link for any browser on the same PC.
  The pending sign-in also offers copy/retry without starting another login.
  Clipboard failures keep the pending sign-in available.

## [0.1.27] - 2026-11-16

- **Codex Account Switcher:** sign in once with ChatGPT, list saved local
  accounts, choose a workspace and switch through Codex's native account API.
  Codex retains and renews credentials; Hub never copies or exports its tokens.
- A switch checks the selected login, workspace and signed-in account. Failed
  verification attempts to restore the previous selection and reports failures.
- Browser sign-in supports cancellation and timeout. Hiding an inactive account
  affects only Hub; its saved sign-in remains in Codex.
- Requires a Codex version supporting `accountSession/list` and
  `accountSession/switch` (verified with 0.167.0-alpha.3.2). Reopen the desktop
  app or start a new CLI session if an existing process still shows the old account.

## [0.1.26] - 2026-11-15

- **Antigravity 2.32:** account switching keeps the sign-in Antigravity stores.
  From 2.32 the app keeps it in Windows Credential Manager with an
  `id_token_email` and fields of its own (`writer_id`, plan tier); the hub used
  to rewrite the entry without them. A switch now keeps Antigravity's fields,
  sets the email to the new account and drops the previous account's plan.
- The old `jetski-standalone-oauth-token` file is only updated when it still
  exists; the hub no longer creates it, since Antigravity 2.32 can restore its
  sign-in from a leftover copy.
- The signed-in email is read from the stored sign-in when there is no ID
  token, instead of asking Google each time.
- Close and force-close also end the separately installed Antigravity IDE.

## [0.1.25] - 2026-10-17

- **Update button:** when GitHub has a newer release, the top bar offers it.
  The installer is downloaded only from this repository's releases, checked
  against `SHA256SUMS.txt`, and started; the app closes while it runs.
- **Antigravity 2.24:** account switching works again. The new Antigravity
  app keeps its sign-in in `~/.gemini/jetski-standalone-oauth-token`; switching
  now writes there and restarts its language server. A database constraint
  also made every switch (and auto-switch) report an error after it had
  already switched; fixed.
- **Gemini 4 Argon:** the separate Argon weekly allowance and its reset time
  are shown on each pool account. Antigravity reports it only for the signed-in
  account, so other accounts show their last reading and when it was taken.
- **Copy sign-in link:** a small link button next to "Sign in with Google"
  copies the Google sign-in address to open it in any browser or profile on
  this PC. The page shown after signing in matches the app's look.
- Opening the app while it is already running brings the open window forward
  instead of starting a second copy.
- Rose-burgundy backdrop, bars, scrollbars, menus and switches replace the
  remaining purple and grey.

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
