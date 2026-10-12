# Release incidents

What to do when a published desktop release is broken or cannot be trusted. Key handling is in [updater-keys.md](updater-keys.md).

## Detection

- The nightly workflow fetches `https://updates.mivia.ai/stable/latest.json` and `/beta/latest.json`, compares them with what `channels/*.json` on `main` renders and verifies the `.sig`. A difference means the manifest was changed outside a reviewed pull request, frozen, or drifted.
- After every publish, `release-published.yml` verifies the build provenance attestation of each asset and the updater signature of the installer. A failure stops the channel pull request.
- Telemetry and problem reports show `pending_install` failures: an update was started but the new version never came up.
- Support tickets and the customer contact address.

## Broken build

The release is genuine but misbehaves.

1. Point the channel back to the last good release: run `promote.yml` with that tag and channel (`skip_soak` for a rollback), review and merge the pull request. Installs that have not updated yet stop seeing the bad version. The app never downgrades, so installs that already updated stay on it.
2. If the bad version must stop working against the server, add it to `blockedDesktopVersions` in the server's `/api/version` settings, or raise `minDesktopVersion` above it. Affected installs show "Update required" and install the next signed update as soon as no upload is running.
3. Fix forward: cut a new beta, let it soak, promote it.

## Malicious build or key A leak

Treat any release that was not built from reviewed code on `main`, and any exposure of key A, as an attack.

1. Remove the `release` environment secret `TAURI_SIGNING_PRIVATE_KEY` so CI cannot sign anything more.
2. Point both channels at the last trusted release (as above) and add the bad version to `blockedDesktopVersions`.
3. Switch to key B and replace key A as described in [updater-keys.md](updater-keys.md).
4. Revoke every access key the server saw with the bad version in its User-Agent, and ask the affected customers to create new ones.
5. Tell customers what happened, which versions are affected, and how to reinstall a trusted version silently for the current user: `"MiViA Desktop_<version>_x64-setup.exe" /S`.
6. Once releases are Authenticode-signed, also ask the certificate provider to revoke the signing certificate with an effective time before the bad signing.

## After the incident

Write down the timeline, the cause and what changes, and rehearse the rollback with the next release checklist.
