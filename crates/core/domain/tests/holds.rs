use mivia_domain::hold::{
    Access, Gate, HoldReason, KeyProblem, LicenseScope, NetworkBlock, SERVICE_UNAVAILABLE_POLL_MS,
    can_claim,
};
use mivia_domain::retry::RetryPolicy;
use proptest::prelude::*;
use proptest::sample::subsequence;

const GATES: [Gate; 7] = [
    Gate::Upload,
    Gate::Submit,
    Gate::StatusPoll,
    Gate::ReportRequest,
    Gate::ReportDownload,
    Gate::Catalogue,
    Gate::Labels,
];

const SPEC_TABLE: &[(&str, &str)] = &[
    ("Offline", "no no no no no no no"),
    ("ServiceUnavailable", "no no 60s no yes yes yes"),
    ("NetworkBlocked", "no no no no no no no"),
    ("AccessKeyInvalid", "no no no no no no yes"),
    ("LicenseInvalid{Uploads}", "no yes yes yes yes yes yes"),
    ("LicenseInvalid{All}", "no no yes yes yes yes yes"),
    ("ClientTooOld", "no yes yes yes yes yes yes"),
];

fn holds() -> Vec<(HoldReason, &'static str)> {
    let mut all = vec![
        (HoldReason::Offline, "Offline"),
        (HoldReason::ServiceUnavailable, "ServiceUnavailable"),
        (
            HoldReason::LicenseInvalid(LicenseScope::Uploads),
            "LicenseInvalid{Uploads}",
        ),
        (
            HoldReason::LicenseInvalid(LicenseScope::All),
            "LicenseInvalid{All}",
        ),
        (HoldReason::ClientTooOld, "ClientTooOld"),
    ];
    let blocks = [
        NetworkBlock::Tls,
        NetworkBlock::ProxyAuth,
        NetworkBlock::ProxyRefused,
        NetworkBlock::Redirect,
    ];
    all.extend(blocks.map(|block| (HoldReason::NetworkBlocked(block), "NetworkBlocked")));
    let problems = [
        KeyProblem::Invalid,
        KeyProblem::AccountBlocked,
        KeyProblem::AdminScope,
        KeyProblem::MissingScopes,
    ];
    all.extend(problems.map(|problem| (HoldReason::AccessKeyInvalid(problem), "AccessKeyInvalid")));
    all
}

fn spec_access(cell: &str) -> Access {
    match cell {
        "yes" => Access::Allowed,
        "60s" => Access::Throttled {
            min_interval_ms: SERVICE_UNAVAILABLE_POLL_MS,
        },
        _ => Access::Blocked,
    }
}

fn spec_row(name: &str) -> Vec<Access> {
    let (_, cells) = SPEC_TABLE
        .iter()
        .find(|(row, _)| *row == name)
        .expect("row in the spec table");
    cells.split(' ').map(spec_access).collect()
}

#[test]
fn every_hold_and_gate_matches_the_spec_table() {
    for (hold, name) in holds() {
        let actual: Vec<Access> = GATES.iter().map(|&gate| hold.access(gate)).collect();
        assert_eq!(actual, spec_row(name), "{name}");
    }
}

#[test]
fn no_hold_allows_everything() {
    for gate in GATES {
        assert_eq!(can_claim(gate, &[]), Access::Allowed);
    }
}

#[test]
fn probes_follow_the_cadence_table() {
    let probe = |hold: HoldReason| hold.probe().map(|policy| (policy.first_ms, policy.cap_ms));
    assert_eq!(probe(HoldReason::Offline), Some((15_000, 300_000)));
    assert_eq!(
        probe(HoldReason::ServiceUnavailable),
        Some((30_000, 300_000))
    );
    assert_eq!(
        probe(HoldReason::NetworkBlocked(NetworkBlock::Tls)),
        Some((60_000, 900_000))
    );
    assert_eq!(
        probe(HoldReason::AccessKeyInvalid(KeyProblem::Invalid)),
        Some((1_800_000, 1_800_000))
    );
    assert_eq!(
        HoldReason::LicenseInvalid(LicenseScope::All).probe(),
        Some(RetryPolicy::LICENSE_CANARY)
    );
    assert_eq!(probe(HoldReason::ClientTooOld), None);
    assert_eq!(
        probe(HoldReason::AccessKeyInvalid(KeyProblem::MissingScopes)),
        None
    );
}

fn exhaustive_subsets() -> Vec<Vec<HoldReason>> {
    let all: Vec<HoldReason> = holds().into_iter().map(|(hold, _)| hold).collect();
    (0_u32..1 << all.len())
        .map(|mask| {
            all.iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, hold)| *hold)
                .collect()
        })
        .collect()
}

#[test]
fn claim_is_the_strictest_hold_over_every_subset() {
    for subset in exhaustive_subsets() {
        for gate in GATES {
            let blocked = subset
                .iter()
                .any(|hold| hold.access(gate) == Access::Blocked);
            let throttled = subset
                .iter()
                .any(|hold| matches!(hold.access(gate), Access::Throttled { .. }));
            let expected = match (blocked, throttled) {
                (true, _) => Access::Blocked,
                (false, true) => Access::Throttled {
                    min_interval_ms: SERVICE_UNAVAILABLE_POLL_MS,
                },
                (false, false) => Access::Allowed,
            };
            assert_eq!(can_claim(gate, &subset), expected, "{gate:?} {subset:?}");
        }
    }
}

proptest! {
    #[test]
    fn lifting_one_hold_never_releases_another(subset in subsequence(holds(), 0..=12), gate in proptest::sample::select(GATES.to_vec())) {
        let reasons: Vec<HoldReason> = subset.iter().map(|(hold, _)| *hold).collect();
        for index in 0..reasons.len() {
            let mut lifted = reasons.clone();
            let removed = lifted.remove(index);
            prop_assert!(can_claim(gate, &lifted) <= can_claim(gate, &reasons));
            prop_assert!(removed.access(gate) <= can_claim(gate, &reasons));
        }
    }
}
