# History is local only

The desktop history is built solely from the app's own SQLite records; the server job list is neither merged into it nor imported on first run, because server jobs lack series, paths and reports, page by offset only, and include jobs from the web app. Lab users get a CSV export and backups instead of a server-side view.

## Considered Options

- Live merge with server jobs; one-time import of past API jobs; a link into the web app (deferred).
