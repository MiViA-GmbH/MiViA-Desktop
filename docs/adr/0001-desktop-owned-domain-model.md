# Desktop-owned domain model, server terms only at the edge

The desktop models its own Watched folders, Batches, Source files, Computations and Reports, and refers to the server's Image, Job, Result and report records by id only, through one server adapter. Mirroring the server was rejected: server Images are deduplicated by content and have no path, so work before upload, offline work and "batch to report" had nowhere to live. Mixing the two vocabularies broke the v1 client every time the API changed.

## Consequences

- A Computation is not a Job, and a Source file is not an Image. The names stay different on purpose.
- The server's job status `NEW` means the job has finished; the adapter maps it to Done.
- An unknown server status counts as still computing for 24 hours after submit, then the computation fails with a reportable reason. It never fails earlier.
- Folders, batches and history belong to the Account (server address and account id), not to the access key, so rotating the key keeps everything.
