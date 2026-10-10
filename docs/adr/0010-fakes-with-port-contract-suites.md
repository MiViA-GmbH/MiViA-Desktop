# Fakes with port contract suites, no mock library

Every port has an in-memory fake in `mivia-testkit` that records calls. The fake and each real adapter run the same contract suite (one test per case via a macro, and an xtask check that both invoke it). One pure `ServerSim` backs the ServerApi fake, the E2E stub and the server contract suite. `mockall` and other mock libraries are banned by cargo-deny.

## Considered Options

- mockall `#[automock]` expectations: less code at first, but tests pin call sequences and mocks drift from adapters silently.

## Consequences

- Up-front cost of writing fakes and harnesses; fakes cannot drift; interaction checks read call logs instead of expectations.
