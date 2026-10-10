mod bundle;
mod context;
mod crash;
pub mod fuzz;
#[cfg(feature = "logging")]
mod logging;
mod outbox;
mod redact;
mod views;

pub use bundle::{BUNDLE_CAP, Bundle, BundleSpec, Entry, build, read_entry};
pub use context::{DiagnosticsContext, MACHINE_ID_NOT_READ, delete_local_logs, machine_id};
pub use crash::{
    PreviousSession, SessionMarker, classify, now_ms, previous_session, remove_marker, write_marker,
};
#[cfg(feature = "logging")]
pub use logging::{Logging, install_panic_hook, start_logging};
pub use outbox::{Delivery, OUTBOX_EXPIRY_MS, Outbox, Pending, delivery};
pub use redact::{Known, Redactor, report_key};
pub use views::{Covered, pending_view, preview, reference};
