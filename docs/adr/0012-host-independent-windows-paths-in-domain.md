# Host-independent Windows paths in the domain

The domain owns a `WinPath` string newtype with its own splitter and NTFS case folding; `std::path::{Path, PathBuf}` is disallowed in the core crates and conversion happens at the port edge. This lets Windows path rules (path keys, nesting, sanitiser, reserved names) be tested, property-tested and fuzzed identically on Linux CI, where `std::path` would treat `C:\a\B` as one file name.

## Consequences

- Most Rust tests run on cheap Linux runners; Windows CI runs only real-OS adapters.
- Adding macOS or Linux later needs a second path flavour behind the same type.
