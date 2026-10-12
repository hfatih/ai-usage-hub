# Security policy

Do not report secrets by committing them. This repository should never contain API keys, OAuth refresh tokens, provider passwords, browser cookies, or copied session files.

Security-sensitive changes must preserve these invariants:

1. Credentials are stored only through a Windows-protected secret store.
2. Provider access uses the provider's own endpoints: documented APIs and OAuth flows where they exist, otherwise the same local or internal interfaces the provider's own app uses.
3. Unsupported usage remains unavailable; it is not inferred from unrelated local data.
4. Executables are launched directly from validated `.exe` paths without shell interpolation.
5. Logs redact authorization, cookie, token, secret, and API-key fields before persistence.
6. Antigravity's per-process CSRF value may be used only transiently for its loopback-only official application endpoint; it must never be persisted, returned to the UI, or logged.

Current Windows installers are unsigned, and automatic binary updates are disabled. Download releases only from this repository and compare the published SHA-256 checksum. If automatic updates are added later, implement signing and verification before enabling them.
