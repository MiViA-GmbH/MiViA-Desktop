# Use-case tests run on real SQLite; the Store is never faked

Handler and worker tests live in a tests-only crate that wires the application to the real SQLite store and fakes only real IO: server, file system, watcher, clock, credentials, notifier, updater. Each test gets a temporary database file copied from a migrated template, with production pragmas except `synchronous=OFF`. A hand-written Store fake would have to reproduce rollback, unique ledger claims, claim queries and foreign keys across all repositories and would drift from SQLite.

## Considered Options

- In-memory SQLite: the build omits shared cache, so it would need a second connection path that production never uses.

## Consequences

- About a millisecond per test.
- Store-backed tests drive worker ticks directly with a manual clock; tokio's paused time is not used where the store, HTTP or blocking tasks are involved.
