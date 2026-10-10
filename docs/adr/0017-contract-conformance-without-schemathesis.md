# Contract conformance via e2e response validation and a test-server smoke, not Schemathesis

The server validates every e2e API response against its committed snapshot (Ajv). On the client side a live smoke against the test server, run from a developer workstation on the VPN, deserialises real responses (including real result items) into the generated types and records scrubbed fixtures that every pull request deserialises again. Schemathesis is not used in release 1: it consumes license, needs a key on a shared environment, and its extra value (fuzzing for 5xx) is server robustness rather than contract sync.

## Consequences

- Nothing authenticated against a MiViA server runs on GitHub, so the public repository holds no access key.
- Fixture drift is noticed when the smoke is rerun, not nightly; spec drift is still checked nightly against production `/docs/json`.
