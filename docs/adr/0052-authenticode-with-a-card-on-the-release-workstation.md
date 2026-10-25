# Releases are Authenticode-signed with a code signing card on the release workstation

Installers are Authenticode-signed with the maintainer's Certum code signing certificate (Unlimited Consulting Krzysztof Bzowski, Certum Code Signing 2021 CA) on a smart card. Because the app exe and the NSIS uninstaller must be signed while the installer is bundled, packaging moves from CI to the workstation: CI compiles and attests the compile output, `pnpm release:sign` bundles and signs it from that output, and `signed-release.yml` proves the uploaded installer holds the compiled exe before it adds the minisign signature and the attestations. The updater pins the signer by the certificate's O and CN plus the SHA-256 of the issuing CA's public key, with the code signing EKU and a timestamp; it does not pin the root. This replaces the Artifact Signing identity and Microsoft root of ADR 0042.

## Considered Options

- Azure Artifact Signing: signs in CI with no manual step, but needs a paid Azure subscription and an organisation validation of MiViA GmbH that was not done.
- Signing only the finished installer on the workstation: the app inside and the uninstaller would stay unsigned.
- Pinning the root: Windows built a chain to `Certum Trusted Network CA` on one machine and to `Certum Trusted Network CA 2` on another, so a root name pin would refuse valid installers.
- Pinning the issuing CA certificate hash instead of its public key: a reissued or cross-signed CA certificate with the same key would be refused.

## Consequences

- Every release needs the card holder at the workstation for a few minutes and one PIN entry per signed file.
- The workstation never builds the app; CI checks that the signed exe in the installer equals the attested compile output. The NSIS script and its hooks are compiled on the workstation from the checked-out tag, and CI does not rebuild them, so the workstation is trusted for the installer logic.
- A new certificate with the same O, CN and issuing CA key only changes the thumbprint in the script. Any other change needs a bridge release that accepts both signers before the switch.
- The publisher shown by Windows is Unlimited Consulting Krzysztof Bzowski, not MiViA GmbH.
