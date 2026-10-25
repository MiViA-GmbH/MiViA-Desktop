use std::fs::File;
use std::path::Path;

pub const CODE_SIGNING: &str = "1.3.6.1.5.5.7.3.3";

#[derive(Clone, Copy, Debug)]
pub struct SignerPin<'a> {
    pub organization: &'a str,
    pub common_name: &'a str,
    pub intermediate_key: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignerFacts {
    pub timestamped: bool,
    pub ekus: Vec<String>,
    pub organization: String,
    pub common_name: String,
    pub intermediate_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthenticodeError {
    Untrusted(i32),
    NoTimestamp,
    WrongEku,
    WrongSubject,
    WrongIntermediate,
    Io,
}

impl SignerPin<'_> {
    pub const fn is_complete(&self) -> bool {
        !self.organization.is_empty()
            && !self.common_name.is_empty()
            && is_sha256_hex(self.intermediate_key)
    }

    pub fn accepts(&self, facts: &SignerFacts) -> Result<(), AuthenticodeError> {
        if !facts.timestamped {
            return Err(AuthenticodeError::NoTimestamp);
        }
        if !facts.ekus.iter().any(|eku| eku == CODE_SIGNING) {
            return Err(AuthenticodeError::WrongEku);
        }
        if facts.organization != self.organization || facts.common_name != self.common_name {
            return Err(AuthenticodeError::WrongSubject);
        }
        if facts.intermediate_key != self.intermediate_key {
            return Err(AuthenticodeError::WrongIntermediate);
        }
        Ok(())
    }
}

const fn is_sha256_hex(text: &str) -> bool {
    let mut rest = text.as_bytes();
    let mut nonzero = false;
    while let [byte, tail @ ..] = rest {
        if !matches!(byte, b'0'..=b'9' | b'a'..=b'f') {
            return false;
        }
        nonzero |= *byte != b'0';
        rest = tail;
    }
    text.len() == 64 && nonzero
}

pub fn signer_facts(file: &File, path: &Path) -> Result<SignerFacts, AuthenticodeError> {
    crate::ffi::signer_facts(file, path)
}

pub fn verify_authenticode(
    file: &File,
    path: &Path,
    pin: &SignerPin<'_>,
) -> Result<(), AuthenticodeError> {
    pin.accepts(&signer_facts(file, path)?)
}

#[cfg(test)]
mod tests {
    use super::{AuthenticodeError, CODE_SIGNING, SignerFacts, SignerPin};

    const KEY: &str = "e64d532cf987ee0260fda04a8251103cb650f7cf8fc92a7d3865f5d6e8992e50";
    const PIN: SignerPin<'static> = SignerPin {
        organization: "Publisher Ltd",
        common_name: "Publisher Ltd",
        intermediate_key: KEY,
    };

    fn facts() -> SignerFacts {
        SignerFacts {
            timestamped: true,
            ekus: vec!["1.3.6.1.5.5.7.3.2".into(), CODE_SIGNING.into()],
            organization: "Publisher Ltd".into(),
            common_name: "Publisher Ltd".into(),
            intermediate_key: KEY.into(),
        }
    }

    #[test]
    fn accepts_matching_facts() {
        assert_eq!(PIN.accepts(&facts()), Ok(()));
    }

    #[test]
    fn requires_a_timestamp() {
        let facts = SignerFacts {
            timestamped: false,
            ..facts()
        };
        assert_eq!(PIN.accepts(&facts), Err(AuthenticodeError::NoTimestamp));
    }

    #[test]
    fn requires_the_code_signing_eku() {
        let missing = SignerFacts {
            ekus: vec!["1.3.6.1.5.5.7.3.2".into()],
            ..facts()
        };
        let longer = SignerFacts {
            ekus: vec!["1.3.6.1.5.5.7.3.30".into()],
            ..facts()
        };
        let none = SignerFacts {
            ekus: Vec::new(),
            ..facts()
        };
        assert_eq!(PIN.accepts(&missing), Err(AuthenticodeError::WrongEku));
        assert_eq!(PIN.accepts(&longer), Err(AuthenticodeError::WrongEku));
        assert_eq!(PIN.accepts(&none), Err(AuthenticodeError::WrongEku));
    }

    #[test]
    fn requires_the_pinned_subject() {
        let organization = SignerFacts {
            organization: "Other Ltd".into(),
            ..facts()
        };
        let common_name = SignerFacts {
            common_name: "Publisher Ltd ".into(),
            ..facts()
        };
        assert_eq!(
            PIN.accepts(&organization),
            Err(AuthenticodeError::WrongSubject)
        );
        assert_eq!(
            PIN.accepts(&common_name),
            Err(AuthenticodeError::WrongSubject)
        );
    }

    #[test]
    fn requires_the_pinned_intermediate() {
        let facts = SignerFacts {
            intermediate_key: KEY.replace('e', "f"),
            ..facts()
        };
        assert_eq!(
            PIN.accepts(&facts),
            Err(AuthenticodeError::WrongIntermediate)
        );
    }

    #[test]
    fn a_pin_is_complete_only_with_names_and_a_sha256() {
        assert!(PIN.is_complete());
        let empty = SignerPin {
            organization: "",
            ..PIN
        };
        let short = SignerPin {
            intermediate_key: "e64d532c",
            ..PIN
        };
        let upper = SignerPin {
            intermediate_key: "E64D532CF987EE0260FDA04A8251103CB650F7CF8FC92A7D3865F5D6E8992E50",
            ..PIN
        };
        let zero = SignerPin {
            intermediate_key: "0000000000000000000000000000000000000000000000000000000000000000",
            ..PIN
        };
        for pin in [empty, short, upper, zero] {
            assert!(!pin.is_complete(), "{pin:?}");
        }
    }
}
