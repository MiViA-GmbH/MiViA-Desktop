# The client's API snapshot tracks production, not dev

The desktop's OpenAPI snapshot is production `/docs/json`, synced nightly, and a gate fails any pull request or release whose snapshot has something production lacks. The client therefore never ships code against an unreleased endpoint; the price is that server features reach the desktop only after a production release (up to 24 h of lag, or a manual sync).

## Considered Options

- The server CI pushes its snapshot to the desktop repo: no lag, but cross-system write credentials and no extra safety.
- Track the test server: VPN-only, and it would let releases depend on unreleased behaviour.
