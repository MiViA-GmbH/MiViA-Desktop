use mivia_application::fuzz::launch_args;
use mivia_application::launch::{Launch, parse};
use mivia_domain::path::WinPath;
use proptest::prelude::*;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn path(text: &str) -> WinPath {
    WinPath::parse(text).expect("valid path")
}

#[test]
fn no_arguments_is_a_plain_start() {
    assert_eq!(parse(&[]), Launch::default());
}

#[test]
fn minimized_autostart() {
    let launch = parse(&args(&["--minimized"]));
    assert!(launch.minimized && !launch.open_paths);
    assert!(launch.paths.is_empty() && launch.ignored.is_empty());
}

#[test]
fn verb_command_with_paths() {
    let launch = parse(&args(&[
        "--open-paths",
        "--",
        r"C:\Lab\a.tif",
        r"\\server\share\b.png",
    ]));
    assert!(launch.open_paths);
    assert_eq!(
        launch.paths,
        vec![path(r"C:\Lab\a.tif"), path(r"\\server\share\b.png")]
    );
}

#[test]
fn drive_root_trailing_quote_maps_back_to_a_backslash() {
    let launch = parse(&args(&["--open-paths", "--", "D:\""]));
    assert_eq!(launch.paths, vec![path(r"D:\")]);
}

#[test]
fn unknown_flags_relative_paths_and_paths_without_the_flag_are_ignored() {
    let launch = parse(&args(&[
        "--devtools",
        "loose",
        "--open-paths",
        "--",
        r"rel\x",
        r"C:\a\..\b",
        r"C:\ok",
    ]));
    assert_eq!(launch.paths, vec![path(r"C:\ok")]);
    assert_eq!(
        launch.ignored,
        args(&["--devtools", "loose", r"rel\x", r"C:\a\..\b"])
    );
    let without_flag = parse(&args(&["--", r"C:\x"]));
    assert_eq!(without_flag.paths, Vec::<WinPath>::new());
    assert_eq!(without_flag.ignored, args(&[r"C:\x"]));
}

#[test]
fn flags_after_the_separator_are_paths_not_flags() {
    let launch = parse(&args(&["--open-paths", "--", "--minimized"]));
    assert!(!launch.minimized);
    assert_eq!(launch.ignored, args(&["--minimized"]));
}

proptest! {
    #[test]
    fn launch_fuzz_entry_holds(data in prop::collection::vec(any::<u8>(), 0..300)) {
        prop_assert_eq!(launch_args(&data), Ok(()));
    }

    #[test]
    fn every_argument_is_accounted_for(items in prop::collection::vec(prop_oneof![
        Just("--open-paths".to_owned()),
        Just("--minimized".to_owned()),
        Just("--".to_owned()),
        "[A-Za-z]:\\\\[a-z]{1,5}\"?",
        any::<String>(),
    ], 0..12)) {
        let joined = items.join("\0");
        prop_assert_eq!(launch_args(joined.as_bytes()), Ok(()));
    }
}
