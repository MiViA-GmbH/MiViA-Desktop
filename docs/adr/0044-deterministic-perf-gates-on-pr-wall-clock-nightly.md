# Deterministic performance gates block pull requests; wall-clock budgets are enforced nightly

Pull requests are blocked only by performance metrics that do not depend on runner speed: bundle sizes, instruction counts, SQLite plan shape and VM steps, thread and process counts, and a generous idle memory floor. No timing check runs on a pull request. The real wall-clock and memory budgets are enforced by a nightly run on the release build with a two-night breach rule, and a release cannot ship while a `perf-red` issue is open.

## Considered Options

- Everything nightly: no PR cost, but regressions land on `main` and are bisected later.
- Everything on PR with tight thresholds: shared 4-vCPU runners vary 10 to 30 %, so gates would fail at random under a no-retry policy.
- Hosted benchmarking service: adds a third-party app and token to a public repo for a signal gungraun gives in-repo.

## Consequences

Timing regressions reach `main` and are caught the next night. Two harnesses exist (gungraun on Linux for instruction counts, criterion on Windows for wall clock).
