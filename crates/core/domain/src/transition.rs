#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IllegalTransition(pub &'static str);
