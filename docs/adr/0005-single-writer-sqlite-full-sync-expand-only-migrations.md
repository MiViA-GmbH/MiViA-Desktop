# One SQLite writer with synchronous=FULL and expand-only migrations

One writer connection on its own thread runs every write transaction, with `synchronous=FULL`, because the database is the queue and WAL with NORMAL can roll back the last commits on power loss, which would resubmit jobs the server already accepted. Within a schema major, migrations only add (tables, nullable or defaulted columns, indexes), so switching between beta and stable or rolling back an update leaves the app writable; destructive changes bump the major and the minimum reader version. One database holds all accounts; isolation comes from an account scope on every repository, an account-switch barrier and a two-account test, because Dormant folders of another account stay visible.

The app keeps a backup before each migration and a daily `VACUUM INTO` backup (7 daily, 6 monthly). A corrupt database is never replaced by an empty one without asking: the user restores a backup or chooses to start empty.

## Considered Options

- One database file per account: isolation by construction, but cross-account reads for Dormant folders and a second migration set.
- `synchronous=NORMAL`: faster commits, but loses outcomes at a write rate where FULL costs only a few milliseconds.
