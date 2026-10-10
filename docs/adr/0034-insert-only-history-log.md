# Insert-only history log

Every user action and series lifecycle step is appended to `history_event` with time, time zone, the local Windows user name, trigger and before/after values. SQLite triggers make the table insert-only, so amendments stay traceable for ISO 17025 labs; it is exported as `changes.csv` next to a SHA-256 list of the exported files.

## Consequences

- Read models may overwrite state; the log is the audit trail.
- The Windows user name never leaves the machine except in an export the user makes.
