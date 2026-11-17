# Codex account switcher

Approved feature: sign in once per ChatGPT account, keep local sign-ins and switch Codex app/CLI accounts from Hub.

Use Codex's native saved-session API, verified in installed Codex 0.167.0-alpha.3.2, instead of copying auth.json or maintaining a second token vault. Codex owns credential storage and renewal. Hub stores only hidden-session identifiers and existing account/usage metadata.

The Accounts page lists saved logins and their workspaces. Sign in with ChatGPT starts native browser OAuth, supports reopen/cancel and a three-minute timeout. Existing sessions remain managed by Codex. A manual switch validates both login and workspace IDs and reads back the signed-in identity. Failed switching or login finalization attempts to restore the previous selection; failed restoration explicitly asks the user to check the active account.

Hiding an inactive login affects only Hub visibility and clearly states its sign-in remains saved in Codex. Active logins cannot be hidden. Signing in again restores visibility. No token export or automatic quota-based switching.

Do not stop running desktop/CLI work. Existing processes may need reopening before they display the selected identity; never describe shared-session validation as proof that an already-open desktop window changed accounts. Show quota snapshots with their read time and match both workspace ID and email.

Use the existing card design and both interface languages. Add no dependencies. Keep all auth diagnostics and tokens out of the frontend, logs and repository. Report unsupported native protocol with update guidance.

Verification covers workspace identity, wrong selection, switch rollback, post-login query/database errors and failed rollback, cancelled/failed login notification filtering, untrusted URLs, hidden-session metadata storage and UI workspace/snapshot matching. Verify native account listing without changing user sessions. Real two-account A → B → A acceptance requires user logins and is reported separately.

Sources: [Codex authentication](https://developers.openai.com/codex/auth), [app-server protocol](https://developers.openai.com/codex/app-server), installed generated native JSON schemas.
