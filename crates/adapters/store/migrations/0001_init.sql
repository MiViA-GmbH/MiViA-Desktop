CREATE TABLE meta (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  schema_major INTEGER NOT NULL,
  min_reader_major INTEGER NOT NULL,
  revision INTEGER NOT NULL,
  label_sort_v INTEGER NOT NULL
) STRICT;

INSERT INTO meta (id, schema_major, min_reader_major, revision, label_sort_v) VALUES (1, 1, 1, 0, 1);

CREATE TABLE account (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  server_url TEXT NOT NULL,
  server_account_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  active INTEGER NOT NULL DEFAULT 0 CHECK (active IN (0, 1)),
  UNIQUE (server_url, server_account_id)
) STRICT;

CREATE UNIQUE INDEX account_one_active ON account(active) WHERE active = 1;

CREATE TABLE account_hold (
  account_id INTEGER NOT NULL REFERENCES account(id),
  reason TEXT NOT NULL,
  since INTEGER NOT NULL,
  next_probe_at INTEGER,
  PRIMARY KEY (account_id, reason)
) STRICT, WITHOUT ROWID;

CREATE INDEX account_hold_probe ON account_hold(account_id, next_probe_at) WHERE next_probe_at IS NOT NULL;

CREATE TABLE settings (
  id INTEGER PRIMARY KEY,
  scope TEXT NOT NULL CHECK (scope IN ('global', 'account', 'setup_draft')),
  account_id INTEGER REFERENCES account(id),
  version INTEGER NOT NULL,
  json TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX settings_scope ON settings(scope, ifnull(account_id, 0));

CREATE TABLE model_catalogue (
  account_id INTEGER NOT NULL REFERENCES account(id),
  model_id TEXT NOT NULL,
  customization_id TEXT NOT NULL DEFAULT '',
  names TEXT NOT NULL,
  version TEXT,
  available INTEGER NOT NULL CHECK (available IN (0, 1)),
  seen_at INTEGER NOT NULL,
  PRIMARY KEY (account_id, model_id, customization_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE watched_folder (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER NOT NULL REFERENCES account(id),
  path TEXT NOT NULL,
  path_key TEXT NOT NULL,
  report_folder_mode TEXT NOT NULL DEFAULT 'subfolder',
  report_folder_name TEXT,
  report_folder_path TEXT,
  report_folder_rel TEXT,
  report_folder_file_id BLOB,
  probe_result TEXT,
  variant_in_file_name INTEGER NOT NULL DEFAULT 0 CHECK (variant_in_file_name IN (0, 1)),
  active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
  position INTEGER NOT NULL,
  missing_since INTEGER,
  waiting_network_since INTEGER,
  rescan_needed INTEGER NOT NULL DEFAULT 0 CHECK (rescan_needed IN (0, 1)),
  quiet_period INTEGER NOT NULL,
  label_rule TEXT NOT NULL,
  notify_on_report INTEGER NOT NULL DEFAULT 1 CHECK (notify_on_report IN (0, 1)),
  report_settings TEXT NOT NULL,
  removed_at INTEGER
) STRICT;

CREATE INDEX watched_folder_by_account ON watched_folder(account_id, position) WHERE removed_at IS NULL;
CREATE UNIQUE INDEX watched_folder_path ON watched_folder(account_id, path_key) WHERE removed_at IS NULL;

CREATE TABLE folder_model_choice (
  folder_id INTEGER NOT NULL REFERENCES watched_folder(id),
  model_choice_key TEXT NOT NULL,
  model_id TEXT NOT NULL,
  customization_id TEXT,
  position INTEGER NOT NULL,
  PRIMARY KEY (folder_id, model_choice_key)
) STRICT, WITHOUT ROWID;

CREATE TABLE batch (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER NOT NULL REFERENCES account(id),
  origin TEXT NOT NULL CHECK (origin IN ('folder', 'one_off')),
  folder_id INTEGER REFERENCES watched_folder(id),
  batch_key TEXT NOT NULL,
  label TEXT NOT NULL,
  label_user_set INTEGER NOT NULL DEFAULT 0 CHECK (label_user_set IN (0, 1)),
  label_sort BLOB NOT NULL,
  open INTEGER NOT NULL CHECK (open IN (0, 1)),
  closure_reason TEXT,
  opened_at INTEGER NOT NULL,
  closed_at INTEGER,
  last_member_at INTEGER,
  sort_at INTEGER NOT NULL,
  report_settings TEXT NOT NULL,
  files INTEGER NOT NULL DEFAULT 0,
  done INTEGER NOT NULL DEFAULT 0,
  failed INTEGER NOT NULL DEFAULT 0,
  problems INTEGER NOT NULL DEFAULT 0,
  active_count INTEGER NOT NULL DEFAULT 0,
  reports_saved INTEGER NOT NULL DEFAULT 0,
  reports_problem INTEGER NOT NULL DEFAULT 0,
  reports_total INTEGER NOT NULL DEFAULT 0,
  reports_stale INTEGER NOT NULL DEFAULT 0,
  needs_attention INTEGER NOT NULL DEFAULT 0 CHECK (needs_attention IN (0, 1)),
  attention_cleared_at INTEGER,
  attention_cleared_how TEXT CHECK (attention_cleared_how IN ('opened', 'dismissed')),
  problems_changed_at INTEGER,
  app_version TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX batch_open_key ON batch(account_id, batch_key) WHERE open = 1;
CREATE INDEX batch_hist_time ON batch(account_id, sort_at DESC, id DESC);
CREATE UNIQUE INDEX batch_closed_sort ON batch(sort_at) WHERE open = 0;
CREATE INDEX batch_open ON batch(account_id) WHERE open = 1;
CREATE INDEX batch_hist_label ON batch(account_id, label_sort, id);
CREATE INDEX batch_hist_folder ON batch(account_id, folder_id, sort_at DESC, id DESC) WHERE folder_id IS NOT NULL;
CREATE INDEX batch_hist_folder_label ON batch(account_id, folder_id, label_sort, id) WHERE folder_id IS NOT NULL;
CREATE INDEX batch_hist_manual ON batch(account_id, sort_at DESC, id DESC) WHERE origin = 'one_off';
CREATE INDEX batch_hist_problems ON batch(account_id, sort_at DESC, id DESC) WHERE problems > 0 OR reports_problem > 0 OR reports_stale > 0;
CREATE INDEX batch_attention ON batch(account_id, sort_at DESC, id DESC) WHERE needs_attention = 1;

CREATE TABLE batch_model_choice (
  batch_id INTEGER NOT NULL REFERENCES batch(id),
  account_id INTEGER NOT NULL REFERENCES account(id),
  model_choice_key TEXT NOT NULL,
  model_id TEXT NOT NULL,
  customization_id TEXT,
  names TEXT NOT NULL,
  model_version TEXT,
  settings_hash TEXT,
  sort_at INTEGER NOT NULL,
  PRIMARY KEY (batch_id, model_choice_key)
) STRICT, WITHOUT ROWID;

CREATE INDEX bmc_hist ON batch_model_choice(account_id, model_id, sort_at DESC, batch_id DESC);

CREATE TABLE source_file (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER NOT NULL REFERENCES account(id),
  batch_id INTEGER REFERENCES batch(id),
  path TEXT NOT NULL,
  label TEXT NOT NULL,
  size INTEGER NOT NULL,
  mtime INTEGER NOT NULL,
  change_time INTEGER,
  usn INTEGER,
  file_id BLOB,
  fingerprint BLOB CHECK (fingerprint IS NULL OR length(fingerprint) = 32),
  state TEXT NOT NULL,
  reason TEXT,
  copy_of INTEGER REFERENCES source_file(id),
  server_image_id TEXT,
  arrival_seq INTEGER NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  next_attempt_at INTEGER,
  first_busy_at INTEGER
) STRICT;

CREATE INDEX source_file_by_batch ON source_file(batch_id, arrival_seq);
CREATE INDEX source_file_due ON source_file(account_id, state, next_attempt_at);
CREATE INDEX source_file_server_image ON source_file(account_id, server_image_id) WHERE server_image_id IS NOT NULL;

CREATE TABLE submit_attempt (
  key TEXT PRIMARY KEY,
  account_id INTEGER NOT NULL REFERENCES account(id),
  model_choice_key TEXT NOT NULL,
  image_ids TEXT NOT NULL,
  started_at INTEGER NOT NULL,
  server_offset_ms INTEGER,
  outcome TEXT
) STRICT, WITHOUT ROWID;

CREATE TABLE computation (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER NOT NULL REFERENCES account(id),
  batch_id INTEGER NOT NULL REFERENCES batch(id),
  source_file_id INTEGER NOT NULL REFERENCES source_file(id),
  model_choice_key TEXT NOT NULL,
  model_version TEXT,
  result_model_version TEXT,
  state TEXT NOT NULL,
  reason TEXT,
  server_job_id TEXT,
  result_id TEXT,
  reused_from INTEGER REFERENCES computation(id),
  superseded_by INTEGER REFERENCES computation(id),
  submit_attempt_key TEXT REFERENCES submit_attempt(key),
  recovery_due INTEGER,
  cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0, 1)),
  resubmitted INTEGER NOT NULL DEFAULT 0 CHECK (resubmitted IN (0, 1)),
  attempts INTEGER NOT NULL DEFAULT 0,
  next_attempt_at INTEGER,
  next_poll_at INTEGER,
  submitted_at INTEGER,
  done_at INTEGER,
  app_version TEXT NOT NULL,
  server_version TEXT
) STRICT;

CREATE INDEX computation_claim ON computation(state, next_attempt_at) WHERE state IN ('queued', 'submitting', 'computing');
CREATE INDEX computation_poll ON computation(account_id, next_poll_at) WHERE state = 'computing';
CREATE INDEX computation_by_batch ON computation(batch_id, model_choice_key, state);
CREATE INDEX computation_by_source ON computation(source_file_id, model_choice_key);
CREATE INDEX computation_server_job ON computation(server_job_id) WHERE server_job_id IS NOT NULL;
CREATE INDEX computation_by_attempt ON computation(submit_attempt_key) WHERE submit_attempt_key IS NOT NULL;

CREATE TABLE ledger_entry (
  folder_id INTEGER NOT NULL REFERENCES watched_folder(id),
  fingerprint BLOB NOT NULL CHECK (length(fingerprint) = 32),
  model_choice_key TEXT NOT NULL,
  computation_id INTEGER NOT NULL REFERENCES computation(id),
  PRIMARY KEY (folder_id, fingerprint, model_choice_key)
) STRICT, WITHOUT ROWID;

CREATE TABLE ledger_path (
  folder_id INTEGER NOT NULL REFERENCES watched_folder(id),
  path TEXT NOT NULL,
  fingerprint BLOB NOT NULL CHECK (length(fingerprint) = 32),
  size INTEGER NOT NULL,
  mtime INTEGER NOT NULL,
  seen_at INTEGER NOT NULL,
  PRIMARY KEY (folder_id, path)
) STRICT, WITHOUT ROWID;

CREATE INDEX ledger_path_by_fingerprint ON ledger_path(folder_id, fingerprint);

CREATE TABLE report (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER NOT NULL REFERENCES account(id),
  kind TEXT NOT NULL CHECK (kind IN ('batch', 'custom')),
  batch_id INTEGER REFERENCES batch(id),
  model_choice_key TEXT NOT NULL,
  custom_name TEXT,
  order_by TEXT NOT NULL DEFAULT 'natural',
  formats TEXT NOT NULL,
  language TEXT NOT NULL,
  decimal_separator TEXT NOT NULL DEFAULT '.',
  include_images INTEGER NOT NULL DEFAULT 1 CHECK (include_images IN (0, 1)),
  timezone TEXT NOT NULL DEFAULT 'UTC',
  state TEXT NOT NULL,
  stale INTEGER NOT NULL DEFAULT 0 CHECK (stale IN (0, 1)),
  replaces INTEGER REFERENCES report(id),
  replaced_by INTEGER REFERENCES report(id),
  attention_cleared_at INTEGER,
  created_at INTEGER NOT NULL,
  app_version TEXT NOT NULL
) STRICT;

CREATE INDEX report_by_batch ON report(batch_id) WHERE batch_id IS NOT NULL;
CREATE INDEX report_custom ON report(account_id, created_at DESC, id DESC) WHERE kind = 'custom';
CREATE INDEX report_stale_open ON report(account_id) WHERE stale = 1 AND replaced_by IS NULL AND attention_cleared_at IS NULL;

CREATE TABLE report_member (
  report_id INTEGER NOT NULL REFERENCES report(id),
  seq INTEGER NOT NULL,
  computation_id INTEGER NOT NULL REFERENCES computation(id),
  label TEXT NOT NULL,
  PRIMARY KEY (report_id, seq)
) STRICT, WITHOUT ROWID;

CREATE INDEX report_member_by_comp ON report_member(computation_id);

CREATE TABLE report_part (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  report_id INTEGER NOT NULL REFERENCES report(id),
  part_no INTEGER NOT NULL,
  part_count INTEGER NOT NULL,
  format TEXT NOT NULL,
  member_from INTEGER NOT NULL,
  member_to INTEGER NOT NULL,
  server_report_id TEXT,
  state TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  rerequests INTEGER NOT NULL DEFAULT 0,
  requested_at INTEGER,
  next_poll_at INTEGER,
  intended_name TEXT,
  staging_path TEXT,
  sha256 BLOB CHECK (sha256 IS NULL OR length(sha256) = 32),
  size INTEGER,
  file_path TEXT,
  saved_path TEXT,
  file_id BLOB,
  saved_at INTEGER,
  moved_to_replaced_at INTEGER,
  error TEXT,
  app_version TEXT NOT NULL,
  server_version TEXT,
  UNIQUE (report_id, part_no, format)
) STRICT;

CREATE INDEX report_part_due ON report_part(state, next_poll_at) WHERE server_report_id IS NOT NULL;
CREATE INDEX report_part_queue ON report_part(state, id) WHERE server_report_id IS NULL;
CREATE INDEX report_part_server_id ON report_part(server_report_id) WHERE server_report_id IS NOT NULL;
CREATE INDEX report_part_problem ON report_part(state) WHERE state IN ('not_saved', 'failed');

CREATE TABLE pending_one_off (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER REFERENCES account(id),
  spool_ids TEXT NOT NULL,
  paths TEXT NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE history_event (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  at INTEGER NOT NULL,
  tz TEXT NOT NULL,
  windows_user TEXT NOT NULL,
  account_id INTEGER NOT NULL REFERENCES account(id),
  batch_id INTEGER REFERENCES batch(id),
  computation_id INTEGER REFERENCES computation(id),
  report_id INTEGER REFERENCES report(id),
  action TEXT NOT NULL,
  trigger TEXT NOT NULL CHECK (trigger IN ('user', 'folder', 'send_to', 'automatic')),
  before_json TEXT,
  after_json TEXT
) STRICT;

CREATE INDEX history_event_batch ON history_event(batch_id, id);
CREATE INDEX history_event_time ON history_event(account_id, id);

CREATE TRIGGER history_event_no_update BEFORE UPDATE ON history_event BEGIN SELECT RAISE(ABORT, 'history_event is insert-only'); END;
CREATE TRIGGER history_event_no_delete BEFORE DELETE ON history_event BEGIN SELECT RAISE(ABORT, 'history_event is insert-only'); END;

CREATE VIRTUAL TABLE batch_search USING fts5(text, content='', contentless_delete=1, detail=full, tokenize='trigram remove_diacritics 1');

CREATE TABLE telemetry_event (
  seq INTEGER PRIMARY KEY,
  session_id TEXT NOT NULL,
  account_id TEXT,
  recorded_at INTEGER NOT NULL,
  name TEXT NOT NULL,
  category TEXT NOT NULL,
  priority INTEGER NOT NULL,
  payload TEXT NOT NULL,
  batch_id TEXT,
  sent_at INTEGER
) STRICT;

CREATE TABLE telemetry_session (
  session_id TEXT PRIMARY KEY,
  resource TEXT NOT NULL
) STRICT, WITHOUT ROWID;
