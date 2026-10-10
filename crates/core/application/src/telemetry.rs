pub const TELEMETRY_DEFAULT_ON: bool = true;

pub const fn effective(
    policy: Option<bool>,
    user: Option<bool>,
    server_default: Option<bool>,
) -> bool {
    match (policy, user, server_default) {
        (Some(policy), _, _) => policy,
        (None, Some(user), _) => user,
        (None, None, Some(server)) => server,
        (None, None, None) => TELEMETRY_DEFAULT_ON,
    }
}
