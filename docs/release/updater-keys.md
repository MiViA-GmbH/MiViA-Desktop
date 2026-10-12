# Updater keys

The desktop app installs an update only when the installer carries a minisign signature from one of two keys compiled into the app (`crates/adapters/update-sig/src/lib.rs`). The signature's trusted comment must also name the version the update manifest announces, so an old installer cannot be served under a new version number.

| Key | Key id | Use |
| --- | --- | --- |
| A | `72C6517AC1698215` | Signs every beta and stable release in GitHub Actions. |
| B | `79A18F67768994C0` | Recovery key. Stays offline and signs only when A is lost or must be replaced. |

The app tries A first and falls back to B. Test keys exist only behind the `test-keys` feature of `mivia-update-sig`, which the e2e build and the test suites enable. A release build with test keys does not compile.

## Generation

Both keys were generated on 2026-10-12 in Windows Sandbox with networking disabled, with `cargo-tauri signer generate` from tauri-cli 2.12.1, and written straight to USB. Neither key has a password: whoever holds a copy of a private key can sign. Physical custody is the protection.

## Custody

- Key A: the secret `TAURI_SIGNING_PRIVATE_KEY` of the `release` environment in `MiViA-GmbH/MiViA-Desktop`. The environment allows only `main`, with no admin bypass. Only the `package` job of `build-release.yml` reads it, in the environment of the single signing step (`cargo xtask ci-integrity` fails on any other use). The environment has no required reviewers while there is one maintainer; add a reviewer when a second maintainer joins.
- Keys A and B: one copy each on two USB sticks, kept in two separate places. Neither stick is ever plugged into a machine that runs project code or has network access.
- Besides the maintainer, the owner of the MiViA GmbH GitHub organisation can retrieve the sticks. Where the sticks are kept is recorded in the company's internal records, not in this repository.

## Signing in CI

`build-release.yml` builds the installer in `compile` (no secrets), then `package` bundles it, signs it with `scripts/ci/sign-installer.sh` (`cargo-tauri signer sign --password '' --app-version <version>`), checks the signature with `out/xtask.exe update-sig verify`, attests the installer, the `.sig` and the SBOMs, and uploads them. The step fails with a clear message when the secret is missing. `release-published.yml` downloads the published assets, verifies their attestations and runs `cargo xtask update-sig verify` before it opens the channel pull request.

## Yearly backup check

Every October, in Windows Sandbox with networking disabled:

1. Copy both private keys from each stick into the sandbox.
2. Sign a scratch file with each key: `cargo-tauri signer sign -f <key file> --password '' --app-version 0.0.1 scratch.txt`.
3. Verify each signature with the matching public key from the table above, for example with `minisign -V -m scratch.txt -x scratch.txt.sig -P <public key>` after decoding the base64 `.sig`.
4. Note the date and the result in the company's internal records. Replace a stick that fails to read.

## Key B rehearsal (before 2.0.0)

1. Build a beta as usual and download its installer from the draft release.
2. In Windows Sandbox with networking disabled, sign it with key B: `cargo-tauri signer sign -f <B key file> --password '' --app-version <X.Y.Z-beta.N> <setup.exe>`.
3. Replace the `.sig` of a test copy of the release and point a local manifest at it, or publish it to the beta channel.
4. Confirm that an installed beta downloads and installs it, then return the channel to the A-signed release.

## Replacing key A

When key A is lost, leaked, or must be retired:

1. Delete the `release` secret `TAURI_SIGNING_PRIVATE_KEY` at once. CI can no longer sign.
2. Generate a new key A in Windows Sandbox with networking disabled, as above, and store it on two sticks.
3. Put its public key into `KEY_A` in `crates/adapters/update-sig/src/lib.rs` and cut a release. Sign that release offline with key B (see the rehearsal) and upload the `.sig` by hand.
4. Once the release is on stable and most installs have updated, store the new key A as the `release` secret. Releases signed with it are accepted by every install that took step 3.
5. Installs older than step 3 still trust only the old A and B. Keep signing with B until they have updated or been reinstalled.
