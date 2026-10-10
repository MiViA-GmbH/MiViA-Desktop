CREATE TABLE telemetry_counter (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  seq INTEGER NOT NULL,
  day INTEGER NOT NULL,
  day_events INTEGER NOT NULL
) STRICT;

INSERT INTO telemetry_counter (id, seq, day, day_events) VALUES (1, 0, 0, 0);

CREATE INDEX telemetry_unsent ON telemetry_event(seq) WHERE sent_at IS NULL;

CREATE INDEX telemetry_unsent_order ON telemetry_event(priority, seq) WHERE sent_at IS NULL;

CREATE INDEX telemetry_sent ON telemetry_event(sent_at) WHERE sent_at IS NOT NULL;
