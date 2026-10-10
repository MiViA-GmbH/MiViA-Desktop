pub mod clock;
pub mod credentials;
pub mod fs;
pub mod fs_reports;
pub mod reports;
pub mod server;
pub mod watch;

use std::fmt::Debug;

pub type CaseResult = Result<(), String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    CaseInsensitive,
    ReparsePoints,
    ShareModes,
    LongPaths,
}

pub fn run(case: impl Future<Output = CaseResult>) -> CaseResult {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?
        .block_on(case)
}

pub fn check(condition: bool, what: &str) -> CaseResult {
    if condition {
        Ok(())
    } else {
        Err(what.to_owned())
    }
}

pub fn check_eq<T: PartialEq + Debug>(actual: &T, expected: &T, what: &str) -> CaseResult {
    check(
        actual == expected,
        &format!("{what}: got {actual:?}, expected {expected:?}"),
    )
}

#[macro_export]
macro_rules! contract_cases {
    ($suite:ident, $harness:expr, $($case:ident),+ $(,)?) => {
        $(
            #[test]
            fn $case() -> $crate::contract::CaseResult {
                let harness = $harness;
                $crate::contract::run($crate::contract::$suite::$case(&harness))
            }
        )+
    };
}
