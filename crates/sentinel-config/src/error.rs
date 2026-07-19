use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file {path}: {source}")]
    ReadError { path: String, source: std::io::Error },
    #[error("Failed to parse config: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("Config file not found: {0}")]
    NotFound(String),
}
