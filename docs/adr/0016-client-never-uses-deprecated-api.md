# The desktop client never uses deprecated API elements

The client's build drops every operation, parameter and property the server marks `deprecated` before progenitor generates code, so a snapshot sync that deprecates something turns into compile errors at once. This keeps new client code off anything that has a sunset. It does not protect builds released before the deprecation; the server's removal procedure (zero `MiViA-Desktop/` calls for 14 days, or `minDesktopVersion` above the old versions) covers those.

## Consequences

- A deprecated request property must not be required on the server (enforced by the snapshot generator).
- Migrating off a deprecated element is forced within the nightly sync, not at the sunset.
