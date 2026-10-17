# Releasing AI Usage Hub

Each user-facing update gets a version bump, changelog entry, source commit,
Git tag, and GitHub Release. The app's **Update** button reads the latest
GitHub Release, so every release must attach the NSIS installer with a name
ending in `-setup.exe` and a `SHA256SUMS.txt` listing its hash
(`<sha256> *<file name>`). Without a matching checksum the app refuses to run
the download. The tag (`vX.Y.Z`) must be newer than the installed version.

1. Update the same version in `package.json`, `package-lock.json`,
   `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json`.
2. Describe the change in [CHANGELOG.md](CHANGELOG.md).
3. Run `npm ci`, `npm test`, `npm run build`, and `cargo test --locked` from
   `src-tauri`. Before building locally, remap the Windows profile path so
   Rust dependency source paths do not reveal the builder's username:

   ```powershell
   $buildUserPath = [Environment]::GetFolderPath('UserProfile')
   $env:RUSTFLAGS = "--remap-path-prefix=$buildUserPath=C:\build"
   npm run tauri -- build --ci --no-sign
   Remove-Item Env:RUSTFLAGS
   ```

   Omit `--no-sign` only when a signing certificate is configured.
4. Review `git diff --cached` and run a secret scan before committing. Confirm
   that no personal account labels, databases, credentials, logs, or local
   screenshots are staged.
5. Inspect the built installer and executable for personal paths and account
   data; compute a SHA-256 checksum. Commit, create the matching `vX.Y.Z` tag, and push both. Create a GitHub
   Release for that tag with the NSIS setup EXE, its SHA-256 checksum, and a
   concise list of changes. Mark the installer as unsigned until code signing
   is available.
6. Verify the public Release link, downloaded checksum, installer launch,
   and local database preservation on an upgrade.

Never publish a build made with someone else's local SQLite data or provider
credentials. The installer should contain only the compiled app and bundled
assets, while user data remains outside the repository and bundle.
