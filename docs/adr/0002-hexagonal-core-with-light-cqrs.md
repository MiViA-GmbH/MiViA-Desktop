# Hexagonal core with light CQRS, no event sourcing, no actors

The Rust core is a pure synchronous domain, a ports crate and an application crate of plain async use-case functions over a `Cx` struct of `Arc<dyn Port>`s. Commands change state in exactly one SQLite write transaction and do their IO outside it; queries read dedicated read models; events after commit are only wake-up hints and UI change notifications. A layered service layer was rejected because IO inside logic leaves no seams for tests, auth or OS swaps, and the v1 client had exactly that structure.

## Considered Options

- Event sourcing (cqrs-es style): rejected, the server owns job truth, and replay, snapshots and upcasting buy nothing; there is no maintained SQLite event store.
- Actor framework (ractor, kameo): rejected, state already lives in SQLite with one writer; actors would duplicate it in memory and lose atomic multi-aggregate claims such as the content ledger.
- Command structs + handler trait + mediator facade: rejected as ceremony; plain functions give the same separation.

## Consequences

- A new feature is a new use-case function, port or adapter crate; existing callers do not change.
- A future sign-in method is a second `Authorizer` implementation plus a new connect command input.
