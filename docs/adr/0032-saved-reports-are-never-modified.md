# Saved reports are never overwritten, renamed or deleted by the app

Once a report file is saved, the app never modifies, renames or deletes it on its own. An update after Run again produces a new file that names the report it replaces, and the old file stays where it was, marked as having newer results in history. The single exception is user-initiated: after the update is saved, the user may move the old report into a `Replaced` subfolder. That move is all or nothing, verified by hash, logged, and reversible.

## Considered Options

- Overwriting in place, or moving old files automatically: both break files that are open, synced or already sent to a customer, and destroy the audit trail.

## Consequences

A colleague can still send a superseded file from the report folder; history shows its state, and the new report's summary page names the report it replaces.
