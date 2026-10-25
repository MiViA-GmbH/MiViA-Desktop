use std::fs::File;
use std::path::{Path, PathBuf};

use mivia_os::{
    AuthenticodeError, CODE_SIGNING, SignerPin, accepts_notifications, signer_facts, user_idle_ms,
    verify_authenticode,
};

const SIGNED: &str = r"C:\Windows\System32\MpSigStub.exe";
const TEST_SIGNED: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/test-signed.exe"
);
const REVOCATION_OFFLINE: i32 = 0x8009_2013_u32.cast_signed();
const UNTRUSTED_ROOT: i32 = 0x800B_0109_u32.cast_signed();
const RELEASE: SignerPin<'static> = SignerPin {
    organization: "Unlimited Consulting Krzysztof Bzowski",
    common_name: "Unlimited Consulting Krzysztof Bzowski",
    intermediate_key: "e64d532cf987ee0260fda04a8251103cb650f7cf8fc92a7d3865f5d6e8992e50",
};

fn copy_into(dir: &Path, source: &Path) -> PathBuf {
    let target = dir.join("sample.exe");
    std::fs::copy(source, &target).unwrap();
    target
}

fn verify(path: &Path, pin: &SignerPin<'_>) -> Result<(), AuthenticodeError> {
    let file = File::open(path).map_err(|_| AuthenticodeError::Io)?;
    verify_authenticode(&file, path, pin)
}

#[test]
fn unsigned_file_is_untrusted() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_into(dir.path(), &std::env::current_exe().unwrap());
    let result = verify(&path, &RELEASE);
    assert!(
        matches!(result, Err(AuthenticodeError::Untrusted(_))),
        "{result:?}"
    );
}

#[test]
fn file_signed_by_a_test_certificate_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_into(dir.path(), Path::new(TEST_SIGNED));
    assert_eq!(
        verify(&path, &RELEASE),
        Err(AuthenticodeError::Untrusted(UNTRUSTED_ROOT))
    );
}

#[test]
fn signed_file_is_checked_against_the_pin() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_into(dir.path(), Path::new(SIGNED));
    let file = File::open(&path).unwrap();
    let facts = match signer_facts(&file, &path) {
        Err(AuthenticodeError::Untrusted(REVOCATION_OFFLINE)) => return,
        facts => facts.unwrap(),
    };
    assert_eq!(facts.organization, "Microsoft Corporation");
    assert_eq!(facts.common_name, "Microsoft Corporation");
    assert!(
        facts.ekus.iter().any(|eku| eku == CODE_SIGNING),
        "{facts:?}"
    );
    let own = SignerPin {
        organization: &facts.organization,
        common_name: &facts.common_name,
        intermediate_key: &facts.intermediate_key,
    };
    assert!(own.is_complete(), "{facts:?}");
    assert_eq!(own.accepts(&facts), Ok(()));
    assert_eq!(
        verify(&path, &RELEASE),
        Err(AuthenticodeError::WrongSubject)
    );
    let other_intermediate = SignerPin {
        intermediate_key: RELEASE.intermediate_key,
        ..own
    };
    assert_eq!(
        verify(&path, &other_intermediate),
        Err(AuthenticodeError::WrongIntermediate)
    );
}

#[test]
fn idle_facts_are_readable() {
    assert!(user_idle_ms().is_some());
    let _accepts = accepts_notifications();
}
