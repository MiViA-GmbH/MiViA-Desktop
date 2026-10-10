# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's "Report a vulnerability" button on this repository (private vulnerability reporting) or by e-mail to support@mivia.ai. Do not open a public issue.

Include the affected version, the steps to reproduce and the impact you expect. We confirm receipt within 3 working days and keep you informed until a fix is released.

## Supported versions

Only the latest stable release receives security fixes. Updates are delivered through the built-in updater.

## Verifying a release

Every installer is Authenticode-signed by MiViA GmbH and carries a build provenance attestation:

```
gh attestation verify <installer> --repo MiViA-GmbH/<repository> --signer-workflow MiViA-GmbH/<repository>/.github/workflows/build-release.yml --source-ref refs/heads/main --deny-self-hosted-runners
```
