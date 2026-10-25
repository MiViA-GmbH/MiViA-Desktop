# Release signing

Every installer is Authenticode-signed with the code signing certificate of Unlimited Consulting Krzysztof Bzowski, issued by Certum Code Signing 2021 CA and held on a cryptoCertum card. Windows shows that company as the publisher. The card never leaves the maintainer, so the installer is bundled and signed on the release workstation. CI compiles the app, attests what it compiled, and adds the updater signature only after it has checked the signed installer.

## Flow

1. `beta.yml` (beta) or `release-please.yml` (stable) runs `build-release.yml`: performance gate, `Compile`, then `Attest compile output`. The `compile-out` artifact holds the app exe, `xtask.exe`, `cargo-tauri.exe`, `package.json`, the notices and the two SBOMs, and is kept for 7 days.
2. On the workstation, `pnpm release:sign <tag>` checks the compile output, bundles the NSIS installer from it and signs the app exe, the NSIS plugins, the uninstaller and the installer with the card in four signtool calls. It verifies the signatures, uploads the installer to the draft release and starts `signed-release.yml`.
3. `signed-release.yml`:
   - `Check the signed installer` (no secrets) checks the compile output again, extracts `mivia-desktop.exe` from the installer and proves it is the compiled exe plus the bundle type marker and a signature (`xtask authenticode same-app`). It checks the installer and the exe with the updater's signer rule, installs silently on the runner and checks the installed exe and `uninstall.exe` the same way.
   - `Sign for the updater` (environment `release`) downloads the same installer by digest, signs it with minisign key A, checks the signature, attests the installer, the `.sig` and the SBOMs and uploads them.
   - `Verify` checks every release asset's attestation from `signed-release.yml`.
4. Publish the draft as before. `release-published.yml` checks the attestations and the updater signature again and opens the channel pull request.

## Workstation setup

Once per machine:

- Windows 11 with the Windows SDK (`signtool.exe`, 10.0.19041 or newer), 7-Zip with `7z` on `PATH`, the GitHub CLI logged in with write access to the repository, Node and pnpm through proto.
- proCertum CardManager and the card reader. With the card inserted, `Get-ChildItem Cert:\CurrentUser\My\9104B16C1523CEBB0A3605EA176330DD30587AEE` shows the certificate with `HasPrivateKey` true.
- A clone of the repository with `pnpm install` done.

## Steps for each release

1. Start the release as before (run `beta` for a beta, merge the release pull request for stable) and wait until `Compile` and `Attest compile output` are green.
2. In the clone: `git fetch --tags`, then `git checkout <tag>`. The checkout must be clean and exactly at the tag commit.
3. Insert the card and run `pnpm release:sign <tag>`.
4. The command prints `expect 4 card PIN dialogs` and then calls signtool four times through `xtask authenticode sign`, which Tauri runs as its `signCommand`: the app exe, all NSIS plugins in one call, the uninstaller, the installer. Each call can show the Certum PIN dialog. Type the PIN only into that dialog; no script reads, stores or passes it. Nothing is retried: a cancelled or wrong PIN or any signtool error stops the run, and you start it again once the cause is fixed.
5. Read the output before it uploads:
   - `compile-out: run <id> is attested for <tag> at <sha>`
   - `Successfully verified` from `signtool verify /pa /tw` for every file
   - `authenticode: ... accepted, signed by Unlimited Consulting Krzysztof Bzowski` for every file
   - `authenticode: ... holds the image of out\mivia-desktop.exe`
   - `Get-AuthenticodeSignature` with `Valid` for the installer and the exe
6. The command uploads the installer to the draft and starts `signed-release`. Watch the run until `Verify` is green, then publish the draft.

`pnpm release:sign <tag> --dry-run` stops before the upload. `--from <dir>` takes a compile output built locally with `TAG=<tag> node scripts/ci/release-compile.mjs` (the files in `out`, moved elsewhere), skips the attestation check and never uploads; use it to rehearse.

## When something fails

- No compile output for the tag: the artifact is gone after 7 days. Re-run the `Compile` job of the build run; GitHub re-runs `Attest compile output` after it and the run id stays the same.
- A wrong installer was uploaded: `gh release delete-asset <tag> "<installer name>"`, then run the command again.
- A check in `signed-release` fails: read the failing line. Never re-sign by hand or skip a check; fix the cause and run the command again. A `.sig` exists only after every check passed.

## Certificate renewal or replacement

The updater accepts an installer only when its signature is valid and timestamped, the certificate has the code signing EKU, its O and CN equal `SIGNER_ORGANIZATION` and `SIGNER_COMMON_NAME`, and the SHA-256 of the issuing CA's public key equals `SIGNER_INTERMEDIATE_KEY` (all in `crates/adapters/update-sig/src/lib.rs`). The root is not pinned, because Windows builds different Certum chains on different machines. `xtask authenticode verify` and therefore `release:sign` and `signed-release` use the same constants. Each installed version checks a download against the constants compiled into it.

Compute the values of a new certificate with the card inserted:

```powershell
$c = Get-Item Cert:\CurrentUser\My\<thumbprint>
$chain = [System.Security.Cryptography.X509Certificates.X509Chain]::new()
[void]$chain.Build($c)
$c.Subject
$issuer = $chain.ChainElements[1].Certificate
$issuer.Subject
[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($issuer.PublicKey.ExportSubjectPublicKeyInfo())).ToLower()
```

- Same O and CN, same issuing CA key (the usual renewal): change `THUMBPRINT` in `scripts/release-sign.mjs`. Installed versions accept the new certificate.
- Other O or CN, or another issuing CA key: installed versions would refuse every update. First ship a bridge release, still signed with the old certificate, whose check accepts both the old and the new signer. The check accepts one signer today, so the bridge release also changes `SignerPin` to a list. Switch to the new certificate only after most installs run the bridge release, and drop the old signer one release later. The current certificate is valid until 2027-10-25; ship the bridge release well before, because an expired certificate can no longer sign.
- If the private key is lost or compromised, revoke the certificate at Certum (see [incident.md](incident.md)). Timestamped signatures made before the revocation date stay valid.
