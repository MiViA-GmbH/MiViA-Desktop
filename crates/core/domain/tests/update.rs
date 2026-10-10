use mivia_domain::ids::{HOUR_MS, MINUTE_MS, Timestamp};
use mivia_domain::update::{
    Channel, Offer, Refusal, Rules, after_check, next_check, parse, satisfies, too_old, vet,
};

const PREFIX: &str = "https://github.com/MiViA-GmbH/MiViA-API/releases/download/";

fn offer(version: &str, url: &str) -> Offer {
    Offer {
        version: version.to_owned(),
        url: url.to_owned(),
    }
}

fn rules(channel: Channel, blocked: &[String]) -> Rules<'_> {
    Rules {
        channel,
        blocked,
        prefix: PREFIX,
    }
}

fn good(version: &str) -> Offer {
    offer(
        version,
        &format!("{PREFIX}v{version}/MiViA.Desktop_{version}_x64-setup.exe"),
    )
}

#[test]
fn a_newer_release_from_the_pinned_prefix_passes() {
    let vetted = vet(&good("2.1.0"), "2.0.0", &rules(Channel::Stable, &[]));
    assert_eq!(vetted, Ok(parse("2.1.0").unwrap()));
    assert!(vet(&good("2.1.0-beta.1"), "2.0.0", &rules(Channel::Beta, &[])).is_ok());
    assert!(vet(&good("2.1.0"), "2.1.0-beta.3", &rules(Channel::Stable, &[])).is_ok());
}

#[test]
fn the_trust_rules_refuse_each_case() {
    let cases = [
        (good("2.0.0"), "2.0.0", Channel::Stable, Refusal::Downgrade),
        (good("1.9.9"), "2.0.0", Channel::Stable, Refusal::Downgrade),
        (
            good("2.0.0-beta.1"),
            "2.0.0",
            Channel::Beta,
            Refusal::Downgrade,
        ),
        (
            good("nonsense"),
            "2.0.0",
            Channel::Stable,
            Refusal::Downgrade,
        ),
        (
            good("2.1.0-beta.1"),
            "2.0.0",
            Channel::Stable,
            Refusal::Prerelease,
        ),
        (
            offer(
                "2.1.0",
                "https://github.com/evil/MiViA-API/releases/download/x.exe",
            ),
            "2.0.0",
            Channel::Stable,
            Refusal::UrlPrefix,
        ),
        (
            offer(
                "2.1.0",
                "http://github.com/MiViA-GmbH/MiViA-API/releases/download/v2.1.0/a.exe",
            ),
            "2.0.0",
            Channel::Stable,
            Refusal::UrlPrefix,
        ),
        (
            offer("2.1.0", PREFIX),
            "2.0.0",
            Channel::Stable,
            Refusal::UrlPrefix,
        ),
        (
            offer("2.1.0", &format!("{PREFIX}v2.1.0/a.exe?x=1")),
            "2.0.0",
            Channel::Stable,
            Refusal::UrlPrefix,
        ),
        (
            offer("2.1.0", &format!("{PREFIX}../../other/a.exe")),
            "2.0.0",
            Channel::Stable,
            Refusal::UrlPrefix,
        ),
    ];
    for (offer, current, channel, refusal) in cases {
        assert_eq!(
            vet(&offer, current, &rules(channel, &[])),
            Err(refusal),
            "{offer:?}"
        );
    }
    let blocked = vec!["2.1.0".to_owned(), "garbage".to_owned()];
    assert_eq!(
        vet(&good("2.1.0"), "2.0.0", &rules(Channel::Stable, &blocked)),
        Err(Refusal::Blocked)
    );
    assert_eq!(
        vet(&good("v2.1.0"), "2.0.0", &rules(Channel::Stable, &blocked)),
        Err(Refusal::Blocked)
    );
}

#[test]
fn too_old_means_below_the_minimum_or_blocked() {
    assert!(too_old("2.0.0", Some("2.1.0"), &[]));
    assert!(too_old("2.1.0-beta.2", Some("2.1.0"), &[]));
    assert!(!too_old("2.1.0", Some("2.1.0"), &[]));
    assert!(!too_old("2.1.0", None, &[]));
    assert!(!too_old("2.1.0", Some("not a version"), &[]));
    assert!(too_old("2.1.0", None, &["2.1.0".to_owned()]));
    assert!(satisfies("2.1.0", Some("2.1.0")));
    assert!(!satisfies("2.0.9", Some("2.1.0")));
    assert!(satisfies("2.0.9", None));
}

#[test]
fn checks_keep_the_hour_gap_and_the_jittered_interval() {
    let start = Timestamp(10 * HOUR_MS);
    let earliest = start.plus_ms(MINUTE_MS);
    assert_eq!(next_check(earliest, None), earliest);
    let recent = Some(start.plus_ms(-10 * MINUTE_MS));
    assert_eq!(next_check(earliest, recent), start.plus_ms(50 * MINUTE_MS));
    let old = Some(start.plus_ms(-2 * HOUR_MS));
    assert_eq!(next_check(earliest, old), earliest);
    assert_eq!(
        after_check(start, 0),
        start.plus_ms(5 * HOUR_MS + 30 * MINUTE_MS)
    );
    assert_eq!(
        after_check(start, u64::try_from(60 * MINUTE_MS).unwrap_or_default()),
        start.plus_ms(6 * HOUR_MS + 30 * MINUTE_MS)
    );
    for random in [1, 7_777, u64::MAX] {
        let next = after_check(start, random).ms_since(start);
        assert!((5 * HOUR_MS + 30 * MINUTE_MS..=6 * HOUR_MS + 30 * MINUTE_MS).contains(&next));
    }
}

#[test]
fn versions_follow_semver_precedence() {
    let ordered = [
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-alpha.beta",
        "1.0.0-beta",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0",
        "1.0.1",
        "1.2.0",
        "2.0.0",
    ];
    for pair in ordered.windows(2) {
        let [low, high] = pair else { continue };
        assert!(parse(low).unwrap() < parse(high).unwrap(), "{low} < {high}");
    }
    assert_eq!(parse("v2.1.0+build.5"), parse("2.1.0"));
    assert_eq!(parse("2.1.0").unwrap().core(), [2, 1, 0]);
    for bad in [
        "",
        "2",
        "2.1",
        "2.1.0.4",
        "2.x.0",
        "2.1.0-",
        "2.1.0-a..b",
        "2.1.0-a_b",
        "+2.1.0",
    ] {
        assert_eq!(parse(bad), None, "{bad}");
    }
}
