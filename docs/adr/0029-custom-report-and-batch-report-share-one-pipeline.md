# Custom reports and batch reports share one pipeline

Besides the automatic report per batch and model choice, the user can build a Custom report from any Done computations in history (at most 1000). Both are the same thing to the app: an ordered list of computations plus snapshotted report settings, split into parts of at most 100 and rendered by the server. A Report therefore no longer always belongs to one batch.

## Consequences

Staleness, queueing, retries and naming apply equally to both.
