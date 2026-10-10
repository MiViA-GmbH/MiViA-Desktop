use mivia_domain::path::{PathError, WinPath, fold};
use proptest::prelude::*;

fn parse(text: &str) -> WinPath {
    WinPath::parse(text).expect("valid path")
}

#[test]
fn drive_and_unc_forms_are_canonical() {
    assert_eq!(parse(r"c:/Data//Probe\").to_string(), r"C:\Data\Probe");
    assert_eq!(parse(r"C:\").to_string(), r"C:\");
    assert_eq!(parse(r"\\server\share").to_string(), r"\\server\share");
    assert_eq!(parse(r"\\server\share\a").to_string(), r"\\server\share\a");
    assert_eq!(parse(r"\\?\C:\long\path"), parse(r"C:\long\path"));
    assert_eq!(parse(r"\\?\UNC\server\share\x"), parse(r"\\server\share\x"));
    assert_eq!(parse(r"C:\a\b.tif").file_name(), Some("b.tif"));
    assert_eq!(parse(r"C:\").file_name(), None);
    assert!(parse(r"\\?\UNC\server\share\x").is_unc());
    assert!(!parse(r"C:\a").is_unc());
}

#[test]
fn relative_and_hostile_inputs_are_refused() {
    let refused = [
        ("", PathError::NotAbsolute),
        (r"relative\x", PathError::NotAbsolute),
        ("C:relative", PathError::NotAbsolute),
        (r"\rooted", PathError::NotAbsolute),
        (r"\\server", PathError::NotAbsolute),
        (r"\\.\pipe\x", PathError::DotComponent),
        (r"C:\a\..\b", PathError::DotComponent),
        (r"C:\a\.\b", PathError::DotComponent),
        (r"C:\a<b", PathError::InvalidCharacter),
        ("C:\\a\u{0}b", PathError::InvalidCharacter),
        (r"C:\x:stream", PathError::InvalidCharacter),
        (r"1:\x", PathError::NotAbsolute),
    ];
    for (text, error) in refused {
        assert_eq!(WinPath::parse(text), Err(error), "{text:?}");
    }
}

#[test]
fn keys_follow_the_ntfs_fold() {
    assert_eq!(parse(r"C:\Ä\a.JPG").key(), parse(r"c:\ä\A.jpg").key());
    assert_ne!(parse(r"C:\straße").key(), parse(r"C:\STRASSE").key());
    assert_eq!(fold("ß"), "ß");
    assert_eq!(fold("éa"), "ÉA");
}

#[test]
fn nesting_respects_component_boundaries() {
    let root = parse(r"C:\Data\Probe");
    assert!(root.contains(&parse(r"c:\data\probe")));
    assert!(root.contains(&parse(r"C:\Data\Probe\sub\x.tif")));
    assert!(!root.contains(&parse(r"C:\Data\Probe2")));
    assert!(!root.contains(&parse(r"D:\Data\Probe\x")));
    assert!(!root.contains(&parse(r"C:\Data")));
    assert!(root.overlaps(&parse(r"C:\Data")));
    assert!(!root.overlaps(&parse(r"C:\Other")));
    let inside = parse(r"C:\Data\Probe\sub\x.tif");
    let expected = ["sub".to_owned(), "x.tif".to_owned()];
    assert_eq!(root.relative_components(&inside), Some(&expected[..]));
    assert_eq!(root.join("y.tif"), Ok(parse(r"C:\Data\Probe\y.tif")));
    assert_eq!(root.join(".."), Err(PathError::DotComponent));
}

fn component() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 _.äÄßéÉ-]{1,10}".prop_filter("dot components", |part| part != "." && part != "..")
}

fn win_path() -> impl Strategy<Value = WinPath> {
    let root = prop_oneof![
        "[a-zA-Z]".prop_map(|letter| format!("{letter}:\\")),
        (component(), component()).prop_map(|(server, share)| format!(r"\\{server}\{share}\")),
    ];
    (root, prop::collection::vec(component(), 0..6))
        .prop_map(|(root, parts)| parse(&format!("{root}{}", parts.join("\\"))))
}

proptest! {
    #[test]
    fn display_round_trips(path in win_path()) {
        prop_assert!(parse(&path.to_string()) == path);
        prop_assert_eq!(parse(&path.to_string().to_lowercase()).key(), path.key());
    }

    #[test]
    fn key_equality_is_mutual_containment(a in win_path(), b in win_path()) {
        let same = a.contains(&b) && b.contains(&a);
        prop_assert_eq!(a.key() == b.key(), same);
    }

    #[test]
    fn nesting_matches_prefix_of_keys(a in win_path(), b in win_path()) {
        let parent = format!("{}\\", a.key().trim_end_matches('\\'));
        let prefix = b.key() == a.key() || b.key().starts_with(&parent);
        prop_assert_eq!(a.contains(&b), prefix);
    }

    #[test]
    fn joined_paths_are_contained(path in win_path(), name in component()) {
        let child = path.join(&name).expect("valid component");
        prop_assert!(path.contains(&child));
        prop_assert!(!child.contains(&path));
    }

    #[test]
    fn parse_never_panics(text in any::<String>()) {
        if let Ok(path) = WinPath::parse(&text) {
            prop_assert_eq!(parse(&path.to_string()), path);
        }
    }
}
