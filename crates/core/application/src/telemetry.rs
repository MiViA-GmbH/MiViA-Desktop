pub const TELEMETRY_DEFAULT_ON: bool = false;

pub const fn effective(policy: Option<bool>, user: Option<bool>) -> bool {
    match (policy, user) {
        (Some(policy), _) => policy,
        (None, Some(user)) => user,
        (None, None) => TELEMETRY_DEFAULT_ON,
    }
}
