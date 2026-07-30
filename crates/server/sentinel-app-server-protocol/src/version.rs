use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ProtocolVersion {
    #[serde(rename = "1.0")]
    V1,
    #[serde(rename = "2.0")]
    #[default]
    V2,
}

impl ProtocolVersion {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::V1 => "1.0",
            Self::V2 => "2.0",
        }
    }

    pub fn supports_feature(&self, feature: &str) -> bool {
        match self {
            Self::V1 => matches!(feature, "core" | "fs" | "command"),
            Self::V2 => true,
        }
    }
}
