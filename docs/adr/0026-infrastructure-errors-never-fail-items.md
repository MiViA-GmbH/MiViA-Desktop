# Infrastructure errors never fail an item

Connection errors, timeouts, 5xx and proxy problems only ever delay work: they raise account-wide holds (Offline, ServiceUnavailable, NetworkBlocked) confirmed by probes, and items retry with persisted full-jitter backoff. An item becomes Failed only for outcomes attributable to that item (rejected image, refused model, contract error, server FAILED, long timeouts). This keeps a server outage from turning a queue of thousands of images into failures, at the cost of items that may wait indefinitely while a hold lasts.

## Consequences

Give-up rules and a problem-report offer when the network blocks the queue for more than 2 hours replace per-item attempt limits for transport errors; repeated stalls of one upload surface as Needs attention without changing its state.
