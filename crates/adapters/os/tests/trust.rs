use std::fs::File;
use std::path::{Path, PathBuf};

use mivia_os::{
    AuthenticodeError, SignerPin, accepts_notifications, user_idle_ms, verify_authenticode,
};

const SIGNED: &str = r"C:\Windows\System32\MpSigStub.exe";
const CODE_SIGNING: &str = "1.3.6.1.5.5.7.3.3";
const MICROSOFT_ROOT: &str = "Microsoft Root Certificate Authority 2010";
const REVOCATION_OFFLINE: i32 = 0x8009_2013_u32.cast_signed();

fn copy_into(dir: &Path, source: &Path) -> PathBuf {
    let target = dir.join("sample.exe");
    std::fs::copy(source, &target).unwrap();
    target
}

fn verify(path: &Path, eku: &str, root: &str) -> Result<(), AuthenticodeError> {
    let file = File::open(path).map_err(|_| AuthenticodeError::Io)?;
    verify_authenticode(&file, path, &SignerPin { eku, root })
}

fn assert_or_offline(
    actual: &Result<(), AuthenticodeError>,
    expected: &Result<(), AuthenticodeError>,
) {
    let offline = Err(AuthenticodeError::Untrusted(REVOCATION_OFFLINE));
    assert!(actual == expected || *actual == offline, "{actual:?}");
}

#[test]
fn unsigned_file_is_untrusted() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_into(dir.path(), &std::env::current_exe().unwrap());
    let result = verify(&path, CODE_SIGNING, MICROSOFT_ROOT);
    assert!(
        matches!(result, Err(AuthenticodeError::Untrusted(_))),
        "{result:?}"
    );
}

#[test]
fn signed_file_is_checked_against_the_pin() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_into(dir.path(), Path::new(SIGNED));
    assert_or_offline(
        &verify(
            &path,
            "1.3.6.1.4.1.311.97.1",
            "Microsoft Identity Verification Root Certificate Authority 2020",
        ),
        &Err(AuthenticodeError::WrongEku),
    );
    assert_or_offline(
        &verify(&path, CODE_SIGNING, "Another Root"),
        &Err(AuthenticodeError::WrongRoot),
    );
    assert_or_offline(&verify(&path, CODE_SIGNING, MICROSOFT_ROOT), &Ok(()));
}

#[test]
fn idle_facts_are_readable() {
    assert!(user_idle_ms().is_some());
    let _accepts = accepts_notifications();
}
