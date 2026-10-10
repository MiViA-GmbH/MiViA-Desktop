# Updates are trusted only with an offline minisign signature, signed version and Authenticode

An update installs only if its minisign signature verifies (key A, fallback cold key B, both compiled in from 2.0.0), the signed version in the trusted comment matches (`requireSignedVersion`), and the downloaded installer passes `WinVerifyTrust` with a timestamp, MiViA GmbH's Artifact Signing identity EKU and a chain to Microsoft's identity verification root. The minisign signature is made by the maintainer on an offline signing device after reading the commits and lockfile changes since the previous release and verifying the build attestation; CI only Authenticode-signs and never holds an updater key. A stolen updater key alone, a tampered `latest.json`, or a compromised GitHub account or CI job alone cannot push code to installed clients or force a downgrade.

## Considered Options

- Updater key as a secret of the release environment (the original design): fully automated, but the same job also holds the Authenticode signing identity, so one compromised account or build dependency produces both signatures and an automatic fleet-wide install.
- A separate CI-held key trusted only on the beta channel: no manual step for betas, but a CI compromise becomes code execution on every beta install.

## Consequences

- Every beta and stable release needs a short manual signing step; the human publish step already existed.
- Stable clients refuse prerelease versions, the download URL must point at this repository's releases, and versions listed in `blockedDesktopVersions` from the server are never installed, so a channel hijack cannot move users to a withdrawn or beta build.
- The client never downgrades; rollback is re-pointing the channel plus fix-forward.
- Download and install are separate calls with our check in between, on the same bytes that are installed.
- A malicious dependency that survives the cooldown and the maintainer's diff read still ships signed.
