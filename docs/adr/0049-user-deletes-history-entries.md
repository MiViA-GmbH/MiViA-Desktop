# The user can delete an image or an image series from history

This reverses the earlier rule that history could only be removed by the uninstaller. In History the user can delete one image of a series, or a whole image series, after a confirmation. Work still running for those images is cancelled in the same write transaction that hides them; uploads, submits and polls that answer later are dropped instead of failing the worker.

Rows are never removed, because the change log still points at them. A deleted series is marked and left out of every history list, search, series page, export and report build. A deleted image moves, with its computations, into a hidden series of its own, so every query that lists a series' members drops it without extra filters. Content-ledger entries of the deleted results are released, so a later copy of the same image is analysed again instead of reusing a deleted result.

The confirmation offers two choices:

- Also delete the saved report files (off by default). Only report parts the app saved itself are deleted, and only when size and SHA-256 still match the recorded save. Every report containing a deleted result is affected, with the other images in it, and is hidden from history. A file that cannot be deleted is logged and reported; it never stops the deletion.
- Also delete on the MiViA server (on by default). Each job is deleted with `DELETE /api/v2/jobs/{id}`; a job the server no longer knows counts as deleted. A job that another kept result reuses is not deleted. Server errors are reported and never stop the local deletion.

The change log stays insert-only. A `deleted` event records what was deleted, its name and the chosen options; a `deletion_finished` event records how many report files and server jobs were deleted or failed. Earlier events keep their content. `changes.csv` of a full export still lists the events of deleted series.

## Considered Options

- Deleting rows: breaks the foreign keys of the insert-only change log and of reused results.
- A deleted flag on every image row: every read model would need its own filter.
- Never touching report files or server jobs: users asked to remove results completely.

## Consequences

- A job the server creates in the moment an image is deleted is not known yet and stays on the server.
- An older app version that opens the database shows deleted entries again.
