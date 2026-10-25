use mivia_domain::hold::{HoldReason, KeyProblem, LicenseScope, NetworkBlock};
use mivia_domain::ids::{HOUR_MS, MINUTE_MS, SECOND_MS};
use mivia_domain::retry::RetryPolicy;
use mivia_ports::auth::AuthError;
use mivia_ports::id_gen::IdGen;
use mivia_ports::server::CatalogueModel;
use mivia_ports::server::ServerError;
use mivia_ports::store::StoreExt;
use mivia_ports::telemetry::{HoldRaised, LiftedBy, ProductEvent, SettledState};
use mivia_testkit::id_gen::SeededIdGen;
use mivia_testkit::server::{Fault, Licence, Op};

use super::line::{Line, with_clock};
use super::world::jpeg;

const API: [Op; 7] = [
    Op::Upload,
    Op::Submit,
    Op::Status,
    Op::Find,
    Op::Exists,
    Op::License,
    Op::Catalogue,
];

fn none_failed(states: &[String]) -> bool {
    states
        .iter()
        .all(|state| state != "failed" && state != "lost")
}

#[tokio::test]
async fn an_unreachable_server_raises_offline_and_work_resumes_when_it_returns() {
    let line = Line::new().await;
    let mut everything = API.to_vec();
    everything.push(Op::Version);
    line.server
        .set_down(&everything, Some(&ServerError::Unreachable));
    line.files(2).await;
    line.run_for(10 * MINUTE_MS, 5 * SECOND_MS).await;
    assert_eq!(line.holds().await, [HoldReason::Offline]);
    assert_eq!(line.file_states().await, ["ready"; 2]);
    let probes = line.server.count(Op::Version);
    assert!(probes < 40, "{probes} probes in 10 minutes");

    line.server.set_down(&everything, None);
    line.run_for(6 * MINUTE_MS, 5 * SECOND_MS).await;
    assert_eq!(line.holds().await, []);
    assert!(line.states().await.iter().all(|state| state == "done"));
    line.finish().await;
}

#[tokio::test]
async fn an_hour_of_service_errors_fails_nothing_and_stays_within_the_request_budget() {
    let line = Line::new().await;
    line.server
        .set_down(&API, Some(&ServerError::Unavailable { status: 503 }));
    line.files(4).await;
    let start = line.now();
    line.run_for(HOUR_MS, 5 * SECOND_MS).await;
    assert_eq!(line.holds().await, [HoldReason::ServiceUnavailable]);
    assert!(none_failed(&line.states().await));
    let requests = line
        .server
        .journal()
        .into_iter()
        .filter(|(_, at)| *at >= start)
        .count();
    assert!(requests < 150, "{requests} requests in one hour");

    line.server.set_down(&API, None);
    line.run_for(10 * MINUTE_MS, 5 * SECOND_MS).await;
    assert_eq!(line.holds().await, []);
    assert!(line.states().await.iter().all(|state| state == "done"));
    line.finish().await;
}

#[tokio::test]
async fn a_service_hold_lifts_when_the_server_answers_without_a_licence() {
    let line = Line::new().await;
    line.server
        .set_down(&API, Some(&ServerError::Unavailable { status: 503 }));
    line.files(4).await;
    line.run_for(HOUR_MS, 5 * SECOND_MS).await;
    assert_eq!(line.holds().await, [HoldReason::ServiceUnavailable]);

    line.server.set_down(&API, None);
    line.server
        .set_down(&[Op::License], Some(&ServerError::LicenseInvalid));
    line.run_for(10 * MINUTE_MS, 5 * SECOND_MS).await;
    assert!(!line.holds().await.contains(&HoldReason::ServiceUnavailable));
    line.finish().await;
}

#[tokio::test]
async fn a_rejected_access_key_holds_work_until_the_key_is_accepted_again() {
    let line = Line::new().await;
    let invalid = ServerError::Unauthorized(KeyProblem::Invalid);
    line.server.set_down(&[Op::Upload], Some(&invalid));
    line.auth
        .set_problem(Some(AuthError::Rejected(KeyProblem::Invalid)));
    line.files(1).await;
    line.run_for(40 * MINUTE_MS, 30 * SECOND_MS).await;
    let invalid_hold = HoldReason::AccessKeyInvalid(KeyProblem::Invalid);
    assert_eq!(line.holds().await, [invalid_hold]);
    assert_eq!(line.server.count(Op::Upload), 1);
    assert!(line.auth.checks() >= 1);

    line.server.set_down(&[Op::Upload], None);
    line.auth.set_problem(None);
    line.run_for(40 * MINUTE_MS, 30 * SECOND_MS).await;
    assert_eq!(line.holds().await, []);
    assert!(line.states().await.iter().all(|state| state == "done"));
    line.finish().await;
}

#[tokio::test]
async fn a_key_without_the_needed_scopes_holds_work_without_asking_again() {
    let line = Line::new().await;
    let missing = ServerError::Unauthorized(KeyProblem::MissingScopes);
    line.server.set_down(&[Op::Submit], Some(&missing));
    line.files(2).await;
    line.run_for(3 * HOUR_MS, 30 * SECOND_MS).await;
    line.pipeline.probe_now().await.unwrap();
    line.run_for(MINUTE_MS, 30 * SECOND_MS).await;
    let held = HoldReason::AccessKeyInvalid(KeyProblem::MissingScopes);
    assert_eq!(line.holds().await, [held]);
    assert_eq!(line.server.count(Op::Submit), 1);
    assert_eq!(line.auth.checks(), 0);
    assert!(none_failed(&line.states().await));
    assert!(
        line.file_states()
            .await
            .iter()
            .all(|state| state == "uploaded")
    );

    line.server.set_down(&[Op::Submit], None);
    let scope = line.world.scope;
    line.world
        .store
        .write(move |tx| tx.holds(scope).release(held))
        .await
        .unwrap();
    line.run_for(10 * MINUTE_MS, 30 * SECOND_MS).await;
    assert_eq!(line.holds().await, []);
    assert!(line.states().await.iter().all(|state| state == "done"));
    line.finish().await;
}

#[tokio::test]
async fn a_license_canary_keeps_the_hold_until_uploads_are_accepted() {
    let line = Line::new().await;
    line.files(1).await;
    with_clock(&line.world.clock, line.pipeline.upload_tick())
        .await
        .unwrap();
    line.server
        .with_sim(|sim| sim.licence = Licence::NoNewImages);
    line.world.drop_file(r"D:\Lab\later.jpg", &jpeg(77));
    line.world.pass(2 * SECOND_MS).await;
    line.run_for(45 * MINUTE_MS, 30 * SECOND_MS).await;
    let held = HoldReason::LicenseInvalid(LicenseScope::Uploads);
    assert_eq!(line.holds().await, [held]);
    assert_eq!(line.file_states().await, ["uploaded", "ready"]);
    let done = line.states().await.iter().filter(|s| *s == "done").count();
    assert_eq!(done, 2, "the uploaded image was not analysed");
    assert!(line.server.count(Op::Upload) >= 3);

    line.server.with_sim(|sim| sim.licence = Licence::Valid);
    line.run_for(35 * MINUTE_MS, 30 * SECOND_MS).await;
    assert_eq!(line.holds().await, []);
    assert!(line.states().await.iter().all(|state| state == "done"));
    line.finish().await;
}

#[tokio::test]
async fn a_flapping_connection_does_not_count_attempts_against_the_files() {
    let line = Line::new().await;
    let mut everything = API.to_vec();
    everything.push(Op::Version);
    line.server
        .set_down(&everything, Some(&ServerError::Unreachable));
    line.files(1).await;
    line.run_for(MINUTE_MS, SECOND_MS).await;
    assert_eq!(line.holds().await, [HoldReason::Offline]);
    for _ in 0..10 {
        line.server.set_down(&[Op::Version], None);
        while !line.holds().await.is_empty() {
            line.run_for(SECOND_MS, SECOND_MS).await;
        }
        line.server
            .set_down(&[Op::Version], Some(&ServerError::Unreachable));
        line.run_for(10 * SECOND_MS, SECOND_MS).await;
        assert_eq!(line.holds().await, [HoldReason::Offline]);
    }
    line.server.set_down(&everything, None);
    assert_eq!(line.upload_attempts().await, [0]);
    line.finish().await;
}

#[tokio::test]
async fn a_small_series_is_not_starved_by_a_large_one() {
    let line = Line::new().await;
    for n in 0..120_u8 {
        line.world
            .drop_file(&format!(r"D:\Lab\big\image_{n:03}.jpg"), &jpeg(n));
    }
    line.world.pass(2 * SECOND_MS).await;
    for n in 0..3_u8 {
        line.world
            .drop_file(&format!(r"D:\Lab\small\image_{n}.jpg"), &jpeg(200 + n));
    }
    line.world.pass(2 * SECOND_MS).await;
    with_clock(&line.world.clock, line.pipeline.upload_tick())
        .await
        .unwrap();
    let series = line.world.series().await;
    let small = series
        .iter()
        .find(|s| s.label.starts_with("small"))
        .unwrap();
    let small_states = line.world.states(small).await;
    assert_eq!(
        small_states.iter().filter(|(_, s)| s == "uploaded").count(),
        1
    );
    line.finish().await;
}

#[test]
fn a_thousand_installs_resuming_together_spread_their_first_probes() {
    let spread = RetryPolicy::probe(30 * SECOND_MS, 30 * SECOND_MS);
    let delays: Vec<i64> = (0..1_000_u64)
        .map(|seed| {
            let random = SeededIdGen::new(seed).uuid().as_u64_pair().1;
            spread.delay_ms(1, random)
        })
        .collect();
    let first = delays.iter().min().copied().unwrap_or_default();
    let last = delays.iter().max().copied().unwrap_or_default();
    assert!(first >= 0 && last <= 30 * SECOND_MS);
    assert!(last - first >= 29 * SECOND_MS, "spread {first}..{last}");
    let early = delays.iter().filter(|d| **d < 10 * SECOND_MS).count();
    assert!(
        (250..420).contains(&early),
        "{early} of 1000 probe in the first 10 s"
    );
}

fn grain_only() -> Vec<CatalogueModel> {
    vec![CatalogueModel {
        model_id: "grain".to_owned(),
        internal_name: "GRAIN".to_owned(),
        name_en: "Grain size".to_owned(),
        name_de: "Korngröße".to_owned(),
        description_en: String::new(),
        description_de: String::new(),
        customizations: Vec::new(),
    }]
}

#[tokio::test]
async fn choices_missing_from_the_catalogue_become_unavailable() {
    let line = Line::new().await;
    line.server.with_sim(|sim| sim.catalogue = grain_only());
    line.files(1).await;
    let available = line.pipeline.refresh_catalogue().await.unwrap().unwrap();
    assert_eq!(available.into_iter().collect::<Vec<_>>(), ["grain:"]);
    let mut states = line.states().await;
    states.sort();
    assert_eq!(states, ["queued", "unavailable"]);
    line.finish().await;
}

#[tokio::test]
async fn a_refused_submit_checks_the_catalogue_before_failing() {
    let line = Line::new().await;
    line.server.with_sim(|sim| sim.catalogue = grain_only());
    let refused = ServerError::Forbidden {
        code: "ACCESS_DENIED".to_owned(),
    };
    line.server
        .fail_next(Op::Submit, Fault::Before(refused.clone()));
    line.server.fail_next(Op::Submit, Fault::Before(refused));
    line.files(1).await;
    line.run_for(MINUTE_MS, SECOND_MS).await;
    let mut states = line.states().await;
    states.sort();
    assert_eq!(states, ["failed", "unavailable"]);
    line.finish().await;
}

#[tokio::test]
async fn check_now_probes_at_once_instead_of_waiting_for_the_backoff() {
    let line = Line::new().await;
    let mut everything = API.to_vec();
    everything.push(Op::Version);
    line.server
        .set_down(&everything, Some(&ServerError::Unreachable));
    line.files(1).await;
    line.run_for(10 * MINUTE_MS, 5 * SECOND_MS).await;
    assert_eq!(line.holds().await, [HoldReason::Offline]);
    line.server.set_down(&everything, None);
    let probes = line.server.count(Op::Version);
    line.pipeline.probe_now().await.unwrap();
    with_clock(&line.world.clock, line.pipeline.probe_tick())
        .await
        .unwrap();
    assert_eq!(line.server.count(Op::Version), probes + 1);
    assert_eq!(line.holds().await, []);
    line.finish().await;
}

#[tokio::test]
async fn holds_and_settled_analyses_are_reported_once_each() {
    let line = Line::new().await;
    let mut everything = API.to_vec();
    everything.push(Op::Version);
    line.server
        .set_down(&everything, Some(&ServerError::Unreachable));
    line.files(2).await;
    line.run_for(10 * MINUTE_MS, 5 * SECOND_MS).await;
    line.server.set_down(&everything, None);
    line.run_for(6 * MINUTE_MS, 5 * SECOND_MS).await;
    let events = line.telemetry.events();
    let raised: Vec<_> = events
        .iter()
        .filter_map(|event| {
            if let ProductEvent::HoldRaised(raised) = event {
                Some(raised)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        raised,
        [&HoldRaised {
            kind: Some(HoldReason::Offline)
        }]
    );
    let lifted: Vec<_> = events
        .iter()
        .filter_map(|event| {
            if let ProductEvent::HoldLifted(lifted) = event {
                Some(lifted)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(lifted.len(), 1);
    assert_eq!(lifted[0].by, Some(LiftedBy::Probe));
    assert!(lifted[0].duration_ms.is_some());
    let settled: Vec<_> = events
        .iter()
        .filter_map(|event| {
            if let ProductEvent::ComputationSettled(settled) = event {
                Some(settled)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(settled.len(), line.states().await.len());
    assert!(
        settled
            .iter()
            .all(|event| event.state == Some(SettledState::Done))
    );
    line.finish().await;
}

#[tokio::test]
async fn a_proxy_sign_in_blocks_the_network_reports_once_and_resolves_the_proxy_before_probes() {
    let line = Line::new().await;
    let mut everything = API.to_vec();
    everything.push(Op::Version);
    line.server
        .set_down(&everything, Some(&ServerError::ProxyAuth));
    line.files(1).await;
    line.run_for(5 * MINUTE_MS, 5 * SECOND_MS).await;
    assert_eq!(
        line.holds().await,
        [HoldReason::NetworkBlocked(NetworkBlock::ProxyAuth)]
    );
    let probes = line.server.count(Op::Version);
    assert!(probes >= 1);
    assert_eq!(line.route.applied.lock().len(), probes);
    let reported = line
        .telemetry
        .events()
        .iter()
        .filter(|event| matches!(event, ProductEvent::ProxyAuthRequired(_)))
        .count();
    assert_eq!(reported, 1);

    line.server.set_down(&everything, None);
    line.run_for(20 * MINUTE_MS, 5 * SECOND_MS).await;
    assert_eq!(line.holds().await, []);
    assert!(line.states().await.iter().all(|state| state == "done"));
    line.finish().await;
}
