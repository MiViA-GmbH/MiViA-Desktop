#![expect(
    clippy::unreachable,
    reason = "tauri's command macro emits unreachable!() for the return type of async commands"
)]

pub(crate) mod actions;
pub(crate) mod history;
pub(crate) mod queries;
pub(crate) mod setup;
