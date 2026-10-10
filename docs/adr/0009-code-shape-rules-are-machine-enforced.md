# Code-shape rules are machine-enforced

No comments in code files (doc comments included), at most two boolean operators per expression, functions of at most 40 lines and 4 parameters, nesting depth 5, file length limits, and `#[expect(..., reason)]` as the only lint suppression are enforced by tools that fail the build: workspace clippy lints, Biome at error with a GritQL plugin, and `cargo xtask source-rules`, one tree-sitter scanner for Rust, TS, CSS, HTML and SQL.

Simple ternaries are allowed. Nested ternaries and hard-to-read ones (a compound condition, a function or multi-line literal in a branch, more than three formatted lines unless each branch is one JSX element) are banned, and the same limits apply to a Rust `if` used as a value. Configuration and data files (TOML, YAML, JSON, Markdown) may keep comments. A rule's rationale lives in `quality/rules.toml` (`cargo xtask why <id>`), not in comments.

## Considered Options

- Two native lexers (rustc lexer + TypeScript scanner): exact, but two implementations and two allow-lists.
- ESLint `no-comments` beside Biome: a second linter for one rule, Rust uncovered.
- Banning every ternary: forces `if` blocks for trivial choices in JSX.
- Review convention: not automated.

## Consequences

- No rustdoc and no doctests; explanation goes into names, types, tests and ADRs.
- Generated files are listed once in `quality/generated.toml`; header-based detection is never used.
- Gates are never loosened to make code pass. Friction is logged in `quality/retro.md` and decided in a retrospective.
