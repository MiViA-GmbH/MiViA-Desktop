# Crate boundaries are enforced by an xtask architecture test, not by convention

The workspace is split into core (domain, ports, application), one crate per adapter, a Tauri shell and test crates. An xtask test reads `cargo metadata` and fails the build when a crate lacks a rule, depends on a crate outside its allow list (normal, dev and build edges), or reaches a banned crate anywhere in its feature-unified transitive closure. Clippy `disallowed-types` and `disallowed-methods` in `crates/core/clippy.toml` keep file IO, clocks, threads, spawning, randomness and `std::path` out of the core. The root `clippy.toml` carries the same thresholds; xtask checks that exactly these two files exist and agree. A command-size check and a canary step that injects one violation per rule complete it, and the crate diagram is generated from the same rule table.

## Consequences

- A cargo feature cannot isolate a dependency (feature unification), so serde and specta are allowed in the ports crate openly and banned only in domain.
- cargo-deny `wrappers` cannot express "tauri only in the shell" without listing every plugin, so cargo-deny handles external bans only.
- Adapters depend on domain and ports, never on application, so editing a use case recompiles no adapter.
- The test stub crate is the one named exception to "test kit only as a dev dependency"; it never enters the shell's dependency closure.
