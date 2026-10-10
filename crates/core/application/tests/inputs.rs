use mivia_application::fuzz::{spool_file, watch_marker};
use mivia_application::handoff::{self, Handoff, MAX_MESSAGE_BYTES};
use mivia_application::spool::{self, MAX_SPOOL_BYTES, SpoolError};
use mivia_application::watch_marker::{MarkerError, parse as parse_marker};
use mivia_domain::path::WinPath;
use proptest::prelude::*;
use uuid::Uuid;

fn path(text: &str) -> WinPath {
    WinPath::parse(text).expect("valid path")
}

#[test]
fn spool_round_trips() {
    let paths = vec![path(r"C:\Lab\a.tif"), path(r"\\server\share\b.png")];
    let bytes = spool::encode(42, &paths, 1).expect("encodes");
    let parsed = spool::parse(&bytes).expect("parses");
    assert_eq!(
        (
            parsed.pid,
            parsed.paths,
            parsed.non_unicode,
            parsed.rejected
        ),
        (42, paths, 1, 0)
    );
}

#[test]
fn spool_rejects_relative_paths_and_keeps_the_rest() {
    let bytes =
        br#"{"v":1,"pid":7,"paths":["rel\\x.png","C:\\a\\..\\b","D:\\ok.png"],"nonUnicode":0}"#;
    let parsed = spool::parse(bytes).expect("parses");
    assert_eq!(parsed.paths, vec![path(r"D:\ok.png")]);
    assert_eq!(parsed.rejected, 2);
}

#[test]
fn spool_refuses_oversize_unknown_fields_and_other_versions() {
    assert_eq!(
        spool::parse(&vec![b' '; MAX_SPOOL_BYTES + 1]),
        Err(SpoolError::TooLarge)
    );
    assert_eq!(
        spool::parse(br#"{"v":1,"pid":1,"paths":[],"nonUnicode":0,"cwd":"C:\\"}"#),
        Err(SpoolError::Malformed)
    );
    assert_eq!(
        spool::parse(br#"{"v":2,"pid":1,"paths":[],"nonUnicode":0}"#),
        Err(SpoolError::Version)
    );
}

#[test]
fn handoff_messages_round_trip_within_the_cap() {
    for message in [
        Handoff::Wake,
        Handoff::Spool(Uuid::from_u128(0x0192_aaaa_bbbb_7ccc_8ddd_eeee_ffff_0000)),
    ] {
        let bytes = handoff::encode(message);
        assert!(bytes.len() <= MAX_MESSAGE_BYTES);
        assert_eq!(handoff::parse(&bytes), Some(message));
    }
}

#[test]
fn handoff_rejects_partial_and_foreign_messages() {
    let full = handoff::encode(Handoff::Spool(Uuid::from_u128(7)));
    for cut in 0..full.len() {
        assert_eq!(handoff::parse(&full[..cut]), None, "prefix {cut}");
    }
    assert_eq!(handoff::parse(&[2, 1]), None);
    assert_eq!(handoff::parse(&[1, 9]), None);
    assert_eq!(handoff::parse(&[full.as_slice(), &[0]].concat()), None);
}

#[test]
fn marker_fields_are_cleaned_and_capped() {
    let long = "x".repeat(200);
    let text = format!(
        r#"{{"markerId":"0123456789ab","machineName":"PC\u202eevil\u0007","windowsUser":"{long}","appVersion":"2.0.0","watchedPathUnc":"\\\\srv\\share\\in","lastSeenMs":5}}"#
    );
    let marker = parse_marker(text.as_bytes()).expect("parses");
    assert_eq!(marker.machine_name, "PCevil");
    assert_eq!(marker.windows_user.chars().count(), 64);
    assert_eq!(marker.watched_path, path(r"\\srv\share\in"));
}

#[test]
fn marker_rejects_bad_ids_paths_and_oversize_files() {
    let with = |id: &str, unc: &str| {
        format!(
            r#"{{"markerId":"{id}","machineName":"m","windowsUser":"u","appVersion":"v","watchedPathUnc":"{unc}","lastSeenMs":0}}"#
        )
    };
    assert_eq!(
        parse_marker(with("0123456789AB", r"C:\\a").as_bytes()),
        Err(MarkerError::Malformed)
    );
    assert_eq!(
        parse_marker(with("0123", r"C:\\a").as_bytes()),
        Err(MarkerError::Malformed)
    );
    assert_eq!(
        parse_marker(with("0123456789ab", "rel").as_bytes()),
        Err(MarkerError::Malformed)
    );
    assert_eq!(
        parse_marker(&vec![b' '; 64 * 1024 + 1]),
        Err(MarkerError::TooLarge)
    );
}

proptest! {
    #[test]
    fn spool_fuzz_entry_holds(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        prop_assert_eq!(spool_file(&data), Ok(()));
    }

    #[test]
    fn spool_fuzz_entry_holds_on_spool_shaped_input(paths in proptest::collection::vec("[ -~]{0,40}", 0..6)) {
        let text = serde_json::json!({"v": 1, "pid": 3, "paths": paths, "nonUnicode": 0}).to_string();
        prop_assert_eq!(spool_file(text.as_bytes()), Ok(()));
    }

    #[test]
    fn marker_fuzz_entry_holds(name in "\\PC{0,100}", user in "\\PC{0,100}") {
        let text = serde_json::json!({
            "markerId": "00112233aabb", "machineName": name, "windowsUser": user,
            "appVersion": "2.0.0", "watchedPathUnc": r"\\srv\share", "lastSeenMs": 1
        }).to_string();
        prop_assert!(parse_marker(text.as_bytes()).is_ok());
        prop_assert_eq!(watch_marker(text.as_bytes()), Ok(()));
    }

    #[test]
    fn handoff_parse_never_accepts_oversize(data in proptest::collection::vec(any::<u8>(), 19..128)) {
        prop_assert_eq!(handoff::parse(&data), None);
    }
}
