use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Empty stream chunk")]
    EmptyStreamChunk,
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Unexpected content block type")]
    UnexpectedContentBlock,
}
