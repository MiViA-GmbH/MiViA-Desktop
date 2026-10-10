# Channel manifests served from updates.mivia.ai, NSIS only, our own manifest script

Release 1 ships only the per-user NSIS installer. The channel files `channels/stable.json` and `channels/beta.json` live on `main`; a promotion is a reviewed pull request that changes one of them, after which GitHub Actions renders `latest.json` per channel and deploys it to GitHub Pages under the custom domain `updates.mivia.ai`. Stable is promoted only after a 48 h beta soak. Releases are immutable, so a mutable pointer cannot live on them, and tauri-action is dropped because compiling (no secrets) and bundling (signing) run in separate jobs.

## Considered Options

- Static `latest.json` on `releases/latest` via tauri-action: conflicts with immutable releases, commit-date "latest" ordering and the secrets split.
- A separate `channels` branch with direct commits: an unreviewed write path to what every client trusts.
- MSI alongside NSIS: per-machine updates need UAC and leave the tray app dead when unattended; betas cannot be MSI (numeric-only pre-release). Deferred until a customer needs it.

## Consequences

- The custom domain keeps the update endpoint stable if the repository is renamed.
