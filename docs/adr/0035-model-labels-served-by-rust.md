# Model labels come from the server at runtime, through Rust

Model and feedback labels (Tolgee namespace `core`) change when new models ship, independently of desktop releases. The app embeds a pulled snapshot and Rust refreshes it from the public `<Server>/api/languages/<lang>` endpoint (ETag, validated, atomically cached per server), then serves it to the webview over IPC; the desktop's own strings are only ever the committed pull.

## Considered Options

- Webview fetch via `i18next-chained-backend`: a second HTTP stack and a CSP hole per server, 120 KB per language in startup JS.
- Snapshot only: new models show the server display name until the next app release.

## Consequences

- `core` never sits in the JS bundle; labels are current without a release; server-side `desktop` keys can never override the app's own strings.
