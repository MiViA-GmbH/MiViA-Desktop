use std::fs::File;
use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub struct SignerPin<'a> {
    pub eku: &'a str,
    pub root: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignerFacts {
    pub timestamped: bool,
    pub ekus: Vec<String>,
    pub root: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthenticodeError {
    Untrusted(i32),
    NoTimestamp,
    WrongEku,
    WrongRoot,
    Io,
}

impl SignerPin<'_> {
    pub fn accepts(&self, facts: &SignerFacts) -> Result<(), AuthenticodeError> {
        if !facts.timestamped {
            return Err(AuthenticodeError::NoTimestamp);
        }
        if !facts.ekus.iter().any(|eku| eku == self.eku) {
            return Err(AuthenticodeError::WrongEku);
        }
        if facts.root != self.root {
            return Err(AuthenticodeError::WrongRoot);
        }
        Ok(())
    }
}

pub fn verify_authenticode(
    file: &File,
    path: &Path,
    pin: &SignerPin<'_>,
) -> Result<(), AuthenticodeError> {
    pin.accepts(&crate::ffi::signer_facts(file, path)?)
}

#[cfg(test)]
mod tests {
    use super::{AuthenticodeError, SignerFacts, SignerPin};

    const PIN: SignerPin<'static> = SignerPin {
        eku: "1.3.6.1.4.1.311.97.1",
        root: "Root A",
    };

    fn facts() -> SignerFacts {
        SignerFacts {
            timestamped: true,
            ekus: vec!["1.3.6.1.5.5.7.3.3".into(), "1.3.6.1.4.1.311.97.1".into()],
            root: "Root A".into(),
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
    fn requires_the_exact_eku() {
        let missing = SignerFacts {
            ekus: vec!["1.3.6.1.5.5.7.3.3".into()],
            ..facts()
        };
        let longer = SignerFacts {
            ekus: vec!["1.3.6.1.4.1.311.97.12".into()],
            ..facts()
        };
        assert_eq!(PIN.accepts(&missing), Err(AuthenticodeError::WrongEku));
        assert_eq!(PIN.accepts(&longer), Err(AuthenticodeError::WrongEku));
    }

    #[test]
    fn requires_the_pinned_root() {
        let facts = SignerFacts {
            root: "Root B".into(),
            ..facts()
        };
        assert_eq!(PIN.accepts(&facts), Err(AuthenticodeError::WrongRoot));
    }
}
