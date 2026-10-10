use mivia_domain::ids::{BatchId, ComputationId, FolderId, ReportId};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Num,
    Flag,
    Uuid,
    Version,
    Enum(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Tag(&'static str),
    Id(Uuid),
    Entity(&'static str, i64),
    Version(Version),
    Num(u64),
    Flag(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

pub trait Value {
    const KIND: FieldKind;
    fn field(&self) -> Field;
}

impl Value for u64 {
    const KIND: FieldKind = FieldKind::Num;
    fn field(&self) -> Field {
        Field::Num(*self)
    }
}

impl Value for bool {
    const KIND: FieldKind = FieldKind::Flag;
    fn field(&self) -> Field {
        Field::Flag(*self)
    }
}

impl Value for Uuid {
    const KIND: FieldKind = FieldKind::Uuid;
    fn field(&self) -> Field {
        Field::Id(*self)
    }
}

impl Value for Version {
    const KIND: FieldKind = FieldKind::Version;
    fn field(&self) -> Field {
        Field::Version(*self)
    }
}

macro_rules! entities {
    ($($type:ident $kind:literal),* $(,)?) => {
        $(
            impl Value for $type {
                const KIND: FieldKind = FieldKind::Uuid;
                fn field(&self) -> Field {
                    Field::Entity($kind, self.0)
                }
            }
        )*
    };
}

entities! {
    FolderId "folder",
    BatchId "batch",
    ReportId "report",
    ComputationId "computation",
}

macro_rules! tag_values {
    ($($type:ty { $($pat:pat => $text:literal),+ $(,)? })*) => {
        $(
            impl Value for $type {
                const KIND: FieldKind = FieldKind::Enum(&[$($text),+]);
                fn field(&self) -> Field {
                    Field::Tag(match self {
                        $($pat => $text),+
                    })
                }
            }
        )*
    };
}

macro_rules! tags {
    ($($name:ident { $($variant:ident $text:literal),+ $(,)? })*) => {
        $(
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub enum $name {
                $($variant),+
            }

            tag_values! { $name { $($name::$variant => $text),+ } }
        )*
    };
}

macro_rules! events {
    ($($variant:ident $name:literal { $($field:ident: $type:ty),+ $(,)? })*) => {
        $(
            #[derive(Clone, Debug, Default, PartialEq, Eq)]
            pub struct $variant {
                $(pub $field: Option<$type>),+
            }

            impl From<$variant> for ProductEvent {
                fn from(event: $variant) -> Self {
                    Self::$variant(event)
                }
            }
        )*

        #[derive(Clone, Debug, PartialEq, Eq)]
        #[must_use]
        pub enum ProductEvent {
            $($variant($variant)),*
        }

        impl ProductEvent {
            pub const CATALOGUE: &'static [(&'static str, &'static [(&'static str, FieldKind)])] = &[
                $(($name, &[$((stringify!($field), <$type as Value>::KIND)),+])),*
            ];

            pub const fn name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $name),*
                }
            }

            pub fn fields(&self) -> Vec<(&'static str, Field)> {
                let mut fields = Vec::new();
                match self {
                    $(Self::$variant(event) => {
                        $(if let Some(value) = &event.$field {
                            fields.push((stringify!($field), value.field()));
                        })+
                    })*
                }
                fields
            }
        }
    };
}

mod catalogue;

pub use catalogue::*;

pub trait TelemetrySink: Send + Sync {
    fn record(&self, event: ProductEvent);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoTelemetry;

impl TelemetrySink for NoTelemetry {
    fn record(&self, _event: ProductEvent) {}
}
