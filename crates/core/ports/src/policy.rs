#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Hive {
    Machine,
    User,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryValue {
    Text(String),
    Expandable(String),
    Number(u32),
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyValue {
    pub hive: Hive,
    pub name: String,
    pub value: RegistryValue,
}
