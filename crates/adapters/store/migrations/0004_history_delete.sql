ALTER TABLE batch ADD COLUMN deleted_at INTEGER;

ALTER TABLE report ADD COLUMN deleted_at INTEGER;

CREATE INDEX computation_reused ON computation(reused_from) WHERE reused_from IS NOT NULL;
