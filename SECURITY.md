# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's "Report a vulnerability" button on this repository (private vulnerability reporting) or by e-mail to support@mivia.ai. Do not open a public issue.

Include the affected version, the steps to reproduce and the impact you expect. We confirm receipt within 3 working days and keep you informed until a fix is released.

## Supported versions

Only the latest stable release receives security fixes. Updates are delivered through the built-in updater.

## Verifying a release

Every installer is built by GitHub Actions from `main`, carries a build provenance attestation and has a minisign signature (`.sig`) that the built-in updater checks. Beta installers are not Authenticode-signed yet, so Windows SmartScreen may warn about them.

```
gh attestation verify <installer> --repo MiViA-GmbH/MiViA-Desktop --signer-workflow MiViA-GmbH/MiViA-Desktop/.github/workflows/build-release.yml --source-ref refs/heads/main --deny-self-hosted-runners
```
