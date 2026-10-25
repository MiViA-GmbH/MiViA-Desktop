# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's "Report a vulnerability" button on this repository (private vulnerability reporting) or by e-mail to support@mivia.ai. Do not open a public issue.

Include the affected version, the steps to reproduce and the impact you expect. We confirm receipt within 3 working days and keep you informed until a fix is released.

## Supported versions

Only the latest stable release receives security fixes. Updates are delivered through the built-in updater.

## Verifying a release

Every app is compiled by GitHub Actions from `main`. The installer is Authenticode-signed by Unlimited Consulting Krzysztof Bzowski (issued by Certum Code Signing 2021 CA), carries a build provenance attestation and has a minisign signature (`.sig`). The built-in updater checks both signatures before it installs an update.

```
gh attestation verify <installer> --repo MiViA-GmbH/MiViA-Desktop --signer-workflow MiViA-GmbH/MiViA-Desktop/.github/workflows/signed-release.yml --source-ref refs/heads/main --deny-self-hosted-runners
```

In PowerShell, `Get-AuthenticodeSignature <installer>` shows `Valid` and the publisher above.
