# Live smoke against the test server

The smoke drives the real server adapter against the MiViA test server and, on request, records the golden fixtures in `crates/adapters/api/tests/fixtures`. An optional [real sign-in](#real-sign-in) covers the sign-in calls. It runs from a developer workstation on the VPN, never in CI, and never against production (it refuses `app.mivia.ai`).

Needs a CI account key on the test server and a folder of sample micrographs named `pearlite.png`, `ferrite.png` and `grain.png`. Put the key and the test server origin into `.env` in the repository root (the file is never committed):

```
MIVIA_CI_ORIGIN=https://<test server>
MIVIA_CI_KEY=<access key without admin rights>
```

Run from Git Bash:

```
set -a && . ./.env && set +a
cargo run -p mivia-server --example live-smoke -- <samples folder>
```

It checks, in order: the key identity, `/api/version` (upload cap, minimum and blocked versions, the reports v2 gate), the licence, the model catalogue with customizations, the model labels in English and German with their ETag, upload with deduplication, submit, job search, status polling until every job is done, job results, a PDF and a CSV report with the v2 fields (summary, part, omitted, labels, decimal separator) through to the download, a problem report with a zip bundle, and a gzip telemetry batch. Every line starts with `ok` or the run stops with the failing step.

To refresh the fixtures, pass the fixture folder as a second argument, then format them:

```
cargo run -p mivia-server --example live-smoke -- <samples folder> crates/adapters/api/tests/fixtures
pnpm exec biome format --write crates/adapters/api/tests/fixtures
cargo test -p mivia-api
```

Recording overwrites the `<status>.json` file of each operation and leaves hand-written cases (`<status>-<case>.json`, `errors/`) alone. Responses are scrubbed before they are written: the test host becomes `app.mivia.ai`, any other host becomes `example.invalid`, query strings are dropped, e-mail addresses become `ci@example.invalid`, the account and key names and the key prefix are replaced, ids are kept. The run fails if a response contains the access key. `cargo test -p mivia-api` deserialises every fixture into its type and rejects internal hosts, account addresses and signed links.

The server allows 5 problem reports per hour per account; a recording run sends two.

## Real sign-in

`--sign-in` runs a real sign-in with a device code instead of the checks above. The default run does not change and keeps using the CI key. This run needs only `MIVIA_CI_ORIGIN` and no samples, and it refuses production in the same way. It needs the `by-app` endpoint on the test server and a client with the app id `mivia-desktop`.

```
set -a && . ./.env && set +a
cargo run -p mivia-server --example live-smoke -- --sign-in
```

It prints a code and the address of the device page. Open the page, sign in as a developer account and enter the code. The run then goes through `by-app`, `device/code`, `token`, `userinfo` and `issue`, and it stops with a failing step if the code is denied or expires.

To refresh the fixtures of these calls, pass the fixture folder, then format and test them as above:

```
cargo run -p mivia-server --example live-smoke -- --sign-in crates/adapters/api/tests/fixtures
```

This overwrites `200.json` or `201.json` in `oauthClientByApp`, `oauthDeviceCode`, `oauthToken`, `oauthUserinfo` and `accessKeyIssue`, and `oauthToken/400-pending.json`. Other hand-written cases stay.

Scrubbing also replaces tokens, the device code, the user code, the issued key, its prefix and its name with fixed placeholders. From `userinfo` it keeps only `sub`, `name`, `email` and `email_verified`, with a placeholder name and address. Ids are kept. The run stops, with or without a fixture folder, and does not write the response if anything is left: a value it has already replaced, anything shaped like a token or a key, or an unknown field whose name contains `token`, `secret` or `password`.

The id of the issued key is saved in `target/live-smoke-sign-in-key-id`, and the next run sends it as `replaces`, so repeated runs do not pile up keys on the account. Without that file, for example after `cargo clean`, the run issues a key without `replaces`, and the old key idles out after 90 days or can be removed on the website.
