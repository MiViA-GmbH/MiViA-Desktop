use mivia_application::telemetry::{TELEMETRY_DEFAULT_ON, effective};

#[test]
fn policy_beats_the_user_who_beats_the_server_default_which_beats_the_compiled_default() {
    const { assert!(TELEMETRY_DEFAULT_ON) };
    let table = [
        ((Some(false), Some(true), Some(true)), false),
        ((Some(true), Some(false), Some(false)), true),
        ((None, Some(false), Some(true)), false),
        ((None, Some(true), Some(false)), true),
        ((None, None, Some(false)), false),
        ((None, None, Some(true)), true),
        ((None, None, None), true),
    ];
    for ((policy, user, server), expected) in table {
        assert_eq!(
            effective(policy, user, server),
            expected,
            "{policy:?} {user:?} {server:?}"
        );
    }
}
