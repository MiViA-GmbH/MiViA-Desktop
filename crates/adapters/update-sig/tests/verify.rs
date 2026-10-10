use std::path::PathBuf;

use mivia_domain::update::Refusal;
use mivia_testkit::update::{TEST_UPDATE_KEY_A, TEST_UPDATE_KEY_B, trusted_comment};
use mivia_update_sig::{KEY_A, KEY_B, Slot, TEST_KEYS, verify, verify_with};

const ASSET: &[u8] = b"MZ test installer 2.1.0";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn signed(
    key: &mivia_testkit::update::TestUpdateKey,
    data: &[u8],
    version: Option<&str>,
) -> String {
    key.sign(data, &trusted_comment(version)).unwrap()
}

#[test]
fn the_compiled_keys_are_the_test_keys_until_release() {
    let test = [TEST_UPDATE_KEY_A.public(), TEST_UPDATE_KEY_B.public()];
    let compiled = [Some(KEY_A.to_owned()), Some(KEY_B.to_owned())];
    assert_eq!(test == compiled, TEST_KEYS);
}

#[test]
fn key_a_and_then_key_b_are_accepted() {
    let a = signed(&TEST_UPDATE_KEY_A, ASSET, Some("2.1.0"));
    let b = signed(&TEST_UPDATE_KEY_B, ASSET, Some("v2.1.0"));
    assert_eq!(verify(ASSET, &a, "2.1.0"), Ok(Slot::A));
    assert_eq!(verify(ASSET, &b, "2.1.0"), Ok(Slot::B));
    assert_eq!(
        verify_with(KEY_A, ASSET, &b, "2.1.0"),
        Err(Refusal::Signature)
    );
    assert_eq!(
        verify_with(KEY_B, ASSET, &a, "2.1.0"),
        Err(Refusal::Signature)
    );
}

#[test]
fn bad_signatures_and_versions_are_refused() {
    let a = signed(&TEST_UPDATE_KEY_A, ASSET, Some("2.1.0"));
    assert_eq!(verify(b"other bytes", &a, "2.1.0"), Err(Refusal::Signature));
    assert_eq!(verify(ASSET, &a, "2.1.1"), Err(Refusal::SignedVersion));
    let unversioned = signed(&TEST_UPDATE_KEY_A, ASSET, None);
    assert_eq!(
        verify(ASSET, &unversioned, "2.1.0"),
        Err(Refusal::SignedVersion)
    );
    assert_eq!(
        verify(ASSET, "not base64", "2.1.0"),
        Err(Refusal::Signature)
    );
    assert_eq!(verify(ASSET, "", "2.1.0"), Err(Refusal::Signature));
    let tampered = a.replacen('A', "B", 1);
    assert_eq!(verify(ASSET, &tampered, "2.1.0"), Err(Refusal::Signature));
}

#[test]
fn the_fixture_files_are_reproducible() {
    let asset = std::fs::read(fixture("asset.exe")).unwrap();
    assert_eq!(asset, ASSET);
    let expected = [
        (
            "asset.exe.sig",
            signed(&TEST_UPDATE_KEY_A, ASSET, Some("2.1.0")),
        ),
        (
            "asset-b.exe.sig",
            signed(&TEST_UPDATE_KEY_B, ASSET, Some("2.1.0")),
        ),
        (
            "other.exe.sig",
            signed(&TEST_UPDATE_KEY_A, b"other", Some("2.1.0")),
        ),
    ];
    for (name, signature) in expected {
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(fixture(name), &signature).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(fixture(name)).unwrap(),
            signature,
            "{name}"
        );
    }
}
