use mivia_domain::hold::NetworkBlock;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", tag = "mode")]
pub enum ProxySetting {
    #[default]
    System,
    Off,
    Manual {
        address: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum BlockKind {
    Tls,
    ProxyAuth,
    ProxyRefused,
    Redirect,
}

impl From<NetworkBlock> for BlockKind {
    fn from(block: NetworkBlock) -> Self {
        match block {
            NetworkBlock::Tls => Self::Tls,
            NetworkBlock::ProxyAuth => Self::ProxyAuth,
            NetworkBlock::ProxyRefused => Self::ProxyRefused,
            NetworkBlock::Redirect => Self::Redirect,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct NetworkView {
    pub setting: ProxySetting,
    pub managed: bool,
    pub host: String,
    pub issuer: Option<String>,
    pub block: Option<BlockKind>,
}
