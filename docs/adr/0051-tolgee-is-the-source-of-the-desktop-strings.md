# Tolgee is the source of the desktop strings

Desktop strings are edited only in Tolgee. A nightly job pulls the texts from the public `app.mivia.ai/api/languages/<lang>` (a file the server refreshes when an admin updates the translations) and opens a PR; the repo files are a committed pull. The set of keys follows the code: a new key is pasted into the repo in the feature PR that uses it, and the pull only updates texts. The `core` snapshot of ADR 0035 now holds only `model` and is refreshed by the same job.

## Considered Options

- Repo as the source with a push script: Tolgee is VPN only and needs a write key, so the upload was a manual step that was forgotten.
- Pull from the Tolgee API: needs VPN and a key in CI, and Tolgee uses `{x}` placeholders.
- Pull that also adds and deletes keys: a key added in Tolgee before the code uses it would fail `i18n:check`, and a key not yet published by an admin would be deleted.

## Consequences

- Text changes need no developer; key changes still go through a PR.
- New texts reach the desktop only after an admin update on prod.
- Drift between Tolgee and the repo shows up as the nightly PR.
