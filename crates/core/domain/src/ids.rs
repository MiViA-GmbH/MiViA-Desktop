use std::fmt;

macro_rules! row_id {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
            pub struct $name(pub i64);

            impl fmt::Display for $name {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    self.0.fmt(f)
                }
            }
        )+
    };
}

row_id!(
    AccountId,
    FolderId,
    BatchId,
    SourceFileId,
    ComputationId,
    ReportId,
    ReportPartId,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(pub i64);

impl Timestamp {
    #[must_use]
    pub const fn plus_ms(self, ms: i64) -> Self {
        Self(self.0.saturating_add(ms))
    }

    pub const fn ms_since(self, earlier: Self) -> i64 {
        self.0.saturating_sub(earlier.0)
    }
}

pub const SECOND_MS: i64 = 1_000;
pub const MINUTE_MS: i64 = 60 * SECOND_MS;
pub const HOUR_MS: i64 = 60 * MINUTE_MS;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fingerprint(pub [u8; 32]);

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({self})")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelChoice {
    pub model_id: String,
    pub customization_id: Option<String>,
}

impl ModelChoice {
    pub fn key(&self) -> String {
        format!(
            "{}:{}",
            self.model_id,
            self.customization_id.as_deref().unwrap_or_default()
        )
    }

    pub fn from_key(key: &str) -> Option<Self> {
        let (model_id, customization) = key.split_once(':')?;
        Some(Self {
            model_id: model_id.to_owned(),
            customization_id: (!customization.is_empty()).then(|| customization.to_owned()),
        })
    }
}
