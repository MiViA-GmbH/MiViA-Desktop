use mivia_application::setup::SetupDraft;
use mivia_application::telemetry::{TELEMETRY_DEFAULT_ON, effective};
use mivia_ports::ipc::setup::SetupOption;

#[test]
fn policy_beats_the_user_who_beats_the_compiled_default_which_is_off() {
    const { assert!(!TELEMETRY_DEFAULT_ON) };
    let table = [
        ((Some(false), Some(true)), false),
        ((Some(true), Some(false)), true),
        ((Some(true), None), true),
        ((None, Some(false)), false),
        ((None, Some(true)), true),
        ((None, None), false),
    ];
    for ((policy, user), expected) in table {
        assert_eq!(effective(policy, user), expected, "{policy:?} {user:?}");
    }
}

#[test]
fn a_fresh_wizard_draft_starts_with_telemetry_unticked() {
    assert!(!SetupDraft::default().has(SetupOption::Telemetry));
}
