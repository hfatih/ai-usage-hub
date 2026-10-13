<div align="center">
  <img src="docs/images/logo.svg" alt="AI Usage Hub Logo" width="84" height="84">
  <h1>AI Usage Hub</h1>
  <p><strong>Local-first AI usage dashboard, Antigravity account switcher and local API proxy for Windows.</strong></p>

  <p>
    <strong>Language:</strong> <a href="README.md"><strong>EN</strong></a> | <a href="README.tr.md"><strong>TR</strong></a>
    &nbsp;•&nbsp;
    <a href="https://github.com/Hfatih/ai-usage-hub/releases/latest">Download Release</a> ·
    <a href="#connecting-tools">Connecting tools</a> ·
    <a href="#how-it-works">How it works</a>
  </p>

  <p>
    <img src="https://img.shields.io/badge/Language-TR%20%7C%20EN-blue?style=flat-square" alt="TR | EN">
    <img src="https://img.shields.io/badge/Platform-Windows%2010%2F11-0078D6?logo=windows&style=flat-square" alt="Windows Platform">
    <img src="https://img.shields.io/badge/Framework-Tauri%20v2-24C8DB?logo=tauri&style=flat-square" alt="Tauri v2">
    <img src="https://img.shields.io/badge/Core-Rust-DEA584?logo=rust&style=flat-square" alt="Rust Core">
    <img src="https://img.shields.io/badge/UI-React%2019%20%2B%20TypeScript-3178C6?logo=typescript&style=flat-square" alt="React + TS">
    <img src="https://img.shields.io/badge/Storage-SQLite%20WAL-003B57?logo=sqlite&style=flat-square" alt="SQLite WAL">
    <img src="https://img.shields.io/badge/Telemetry-Zero%20%2F%20100%25%20Local-10B981?style=flat-square" alt="Zero Telemetry">
    <img src="https://img.shields.io/badge/License-MIT-blue?style=flat-square" alt="License MIT">
  </p>
</div>

---




## Overview

**AI Usage Hub** is a small Windows desktop app that shows, in one place, how much of your AI subscriptions you have left. It reads quota from the tools already signed in on your PC (Codex, Claude Code and Claude Desktop, Google Antigravity, OpenCode) and keeps everything in a local database. There is no telemetry and no account with us.

For Google Antigravity it goes further: it keeps a pool of your Google accounts, shows each one's real quota, switches the IDE between them, and can expose them to other tools through a local API proxy.

## Screenshots

| Dashboard |
| :---: |
| ![Dashboard](docs/images/dashboard.jpg) |
| *Provider quotas, reset times and which AI apps are running.* |

| Antigravity account pool |
| :---: |
| ![Antigravity account pool](docs/images/accounts.jpg) |
| *Live IDE session, per-account 5-hour and weekly quota, switching and auto-switch.* |

| Local API proxy |
| :---: |
| ![API proxy](docs/images/api-proxy.jpg) |
| *Proxy address, API key, model rules and copy-paste setup for common tools.* |

Screenshots use the app's built-in preview data.

## Features

### Usage dashboard

- **ChatGPT / Codex:** shared 5-hour and weekly windows with reset times, read from the signed-in Codex app.
- **Claude Desktop / Claude Code:** shared 5-hour and weekly quota and local token totals.
- **Google Antigravity:** Gemini and Claude/GPT quota windows, the Google One AI credit balance, and Antigravity's prompt and flow credits.
- **OpenCode:** connected providers, the free model catalog and local token and cost totals.
- Every value is labeled *Official*, *Local*, *Calculated* or *Unavailable*. The app does not show a number it could not read.
- Local alerts when a quota drops below your thresholds, with optional Windows notifications.
- Tracks how long each AI app has been open today and this week.

### Antigravity account pool

- **Live session detection:** asks the running Antigravity language server which account is signed in; falls back to Windows Credential Manager and Antigravity's local state when the IDE is closed.
- **Real quota for every account:** each pooled account's 5-hour and weekly windows, with reset times, refreshed from Google every ten minutes.
- **Account switching:** writes the chosen account's sign-in to Windows Credential Manager and restarts Antigravity's language server. The hub then checks which account the IDE reports; if it is still the old one, it restores the previous sign-in and tells you.
- **Auto-switch:** moves to another healthy account when the current one falls below your threshold or gets rate limited.
- **Add accounts** with Google sign-in in the browser, by capturing the account currently signed in to the IDE, or by importing JSON.
- **Batch delete**, search, sort, grid and list views, JSON export and import.

### Local API proxy (`http://127.0.0.1:8046`)

- **Three API formats:** OpenAI (`/v1/chat/completions`), Anthropic (`/v1/messages`) and Gemini (`/v1beta/models/...:generateContent`).
- **Image generation:** OpenAI Images format (`/v1/images/generations`), returned as `b64_json`.
- **Live model list:** `/v1/models` and the test panel show the models your account can actually call, read from Google. Real model ids pass through unchanged; familiar names such as `claude-3-7-sonnet` or `gpt-4o` are mapped to the current equivalent.
- **Model rules:** send any model name a tool asks for to a model you choose (`gpt-4*` → a Gemini model, for example).
- **API key:** the proxy rejects requests without the key shown in the app. It listens on `127.0.0.1` only.
- **Failover:** on a 429 or 403 the proxy marks the account and retries with the next healthy one.
- **Haiku downgrade:** requests for Haiku-class models (background tasks such as title generation in Claude Code) go to a Flash model to save Claude quota.

Limits worth knowing: streaming responses are delivered as a single chunk rather than token by token, and one image is generated per request.

> [!WARNING]
> Using pooled Google accounts through a local proxy may violate Google's Terms of Service and could get an account suspended. The usage dashboard does not need the proxy; use the proxy at your own risk.

## How it works

```mermaid
flowchart LR
    subgraph Tools["Your tools"]
        CC["Claude Code"]
        ED["Cursor, Aider, Cline, Continue"]
    end

    subgraph Hub["AI Usage Hub (Tauri 2 + Rust)"]
        PX["Local proxy<br/>127.0.0.1:8046"]
        POOL["Account pool<br/>and switcher"]
        MON["Quota readers<br/>and process monitor"]
        DB[("SQLite<br/>tokens encrypted with DPAPI")]
    end

    subgraph Local["On this PC"]
        AG["Antigravity language server"]
        WCM["Windows Credential Manager"]
        APPS["Codex, Claude, OpenCode"]
    end

    G["Google Cloud Code API"]

    CC -->|"Anthropic format + key"| PX
    ED -->|"OpenAI format + key"| PX
    PX --> POOL
    POOL -->|"quota, models, requests"| G
    POOL -->|"switch account"| WCM
    MON -->|"live session and quota"| AG
    MON --> APPS
    POOL --> DB
    MON --> DB
```

## Connecting tools

Copy the API key from **API Proxy** in the app and use it wherever `YOUR_KEY` appears. The app shows these snippets with your key already filled in.

**Claude Code**

```powershell
$env:ANTHROPIC_BASE_URL="http://127.0.0.1:8046"
$env:ANTHROPIC_API_KEY="YOUR_KEY"
claude
```

**Cursor, Roo Code, Cherry Studio and other OpenAI-compatible tools**

- Base URL: `http://127.0.0.1:8046/v1`
- API key: `YOUR_KEY`
- Model: any id from the app's model list, or a familiar name such as `claude-3-7-sonnet`

**Aider**

```powershell
$env:OPENAI_API_BASE="http://127.0.0.1:8046/v1"
$env:OPENAI_API_KEY="YOUR_KEY"
aider --model openai/claude-3-7-sonnet
```

**Image generation**

```powershell
curl http://127.0.0.1:8046/v1/images/generations -H "Authorization: Bearer YOUR_KEY" -H "Content-Type: application/json" -d '{\"prompt\": \"a pink star on a dark background\"}'
```

## Download and installation

1. Download the Windows installer from the [latest release](https://github.com/Hfatih/ai-usage-hub/releases/latest).
2. Run it. It installs for the current Windows user and does not need administrator rights.
3. Start **AI Usage Hub**. Accounts appear once the matching tool is signed in on this PC.
4. Turkish is available under **Settings → Language**.

> [!NOTE]
> Windows 10/11 and Microsoft Edge WebView2 are required. The installer is not code-signed, so Windows SmartScreen may warn on first run; each release lists a SHA-256 checksum you can compare. There is no auto-update: install a newer release over the old one.

Upgrading from 0.1.21 or earlier: the proxy now requires an API key. Tools configured with the old placeholder key need the new key from the API Proxy page.

## Building from source

Requirements: Windows 10/11 x64, [Node.js](https://nodejs.org/) 20+, the [Rust MSVC toolchain](https://rustup.rs/) and Microsoft C++ Build Tools.

```powershell
git clone https://github.com/Hfatih/ai-usage-hub.git
cd ai-usage-hub
npm ci
npm run tauri dev
```

Production installer (written to `src-tauri/target/release/bundle/nsis/`):

```powershell
npm run tauri build
```

Tests:

```powershell
npm test
npm run build
cd src-tauri
cargo test --locked
```

## Data and security

- **Where data lives:** `%APPDATA%\com.aiusagehub.desktop\ai-usage-hub.sqlite3`, on this PC only.
- **No passwords:** the app never asks for or stores a Google, OpenAI or Anthropic password.
- **Sign-in tokens:** Codex and Claude keep their own credentials; the hub stores only labels and usage for them. For Antigravity pool accounts it stores Google OAuth tokens, encrypted with Windows DPAPI for your Windows user. A copied database cannot be read under another Windows user.
- **Exports:** an exported account JSON contains the tokens in plain text. Treat it like a password.
- **Proxy:** listens on `127.0.0.1` only and requires the API key unless you turn that off.
- **No telemetry:** the app talks only to the providers' own endpoints.
- The **Security** page in the app shows what is stored and lets you clear usage history. To report a vulnerability, see [SECURITY.md](SECURITY.md).

## License and disclaimer

Released under the [MIT License](LICENSE).

AI Usage Hub is an independent open-source project and is not affiliated with Google, OpenAI or Anthropic. It relies on undocumented local and internal interfaces that can change without notice. Product names are trademarks of their respective owners.
