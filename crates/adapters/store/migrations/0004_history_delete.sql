ALTER TABLE batch ADD COLUMN deleted_at INTEGER;

ALTER TABLE report ADD COLUMN deleted_at INTEGER;

CREATE INDEX computation_reused ON computation(reused_from) WHERE reused_from IS NOT NULL;

ALTER TABLE batch ADD COLUMN delete_on_server INTEGER NOT NULL DEFAULT 0 CHECK (delete_on_server IN (0, 1));

CREATE TABLE server_cleanup (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER NOT NULL REFERENCES account(id),
  kind TEXT NOT NULL CHECK (kind IN ('job', 'image')),
  server_id TEXT NOT NULL,
  UNIQUE (account_id, kind, server_id)
) STRICT;
