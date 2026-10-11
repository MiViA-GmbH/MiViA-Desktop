# Live smoke against the test server

The smoke drives the real server adapter against the MiViA test server and, on request, records the golden fixtures in `crates/adapters/api/tests/fixtures`. It runs from a developer workstation on the VPN, never in CI, and never against production (it refuses `app.mivia.ai`).

Needs a CI account key on the test server and a folder of sample micrographs named `pearlite.png`, `grain.png`, `ferrite.png` and `carbides.png`. Put the key and the test server origin into `.env` in the repository root (the file is never committed):

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
