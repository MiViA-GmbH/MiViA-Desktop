# The database is the queue; workers are level-triggered

All pipeline work (uploads, submits, polls, reports) is rows in SQLite. A worker takes semaphore permits, claims exactly that many due rows, does the IO and records the outcome in one transaction. Channels and events only wake workers; every worker re-derives due work from SQL on start, on wake-up, on bus lag and on a capped sleep. Only one process ever opens the database, so claims are fenced in memory: each worker keeps the ids it has in flight and excludes them from its claim query, and there are no lease columns. Rules that must follow a state change (report readiness, staleness, report creation at batch closure) run as domain policies inside the same transaction.

An in-process event bus as the carrier of correctness was rejected: events published after commit are lost on a crash, a panic or broadcast lag, which would leave batches stuck without any error.

## Consequences

- Retry times are stored as wall-clock `next_attempt_at` and clamped when the clock jumps or the machine resumes.
- Recovery after a crash or a worker restart is per state: Uploading goes back to Ready, Submitting goes to recovery (adopt a matching server job or resubmit), Computing is polled at once.
- Claim predicates are built from domain state constants, and a property test pins the SQL and domain predicates together.
