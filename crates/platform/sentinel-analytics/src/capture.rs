use std::path::PathBuf;
use crate::events::TrackEventRequest;

/// Where processed analytics events are dispatched.
#[derive(Debug, Clone)]
pub enum AnalyticsDestination {
    /// Send events to a remote HTTP endpoint.
    Http {
        url: String,
    },
    /// Write events as newline-delimited JSON to a local file.
    CaptureFile {
        path: PathBuf,
    },
    /// Discard all events (no-op).
    Null,
}

impl AnalyticsDestination {
    pub async fn dispatch(&self, events: &[TrackEventRequest]) -> Result<(), CaptureError> {
        match self {
            Self::Http { url } => dispatch_http(url, events).await,
            Self::CaptureFile { path } => dispatch_file(path, events).await,
            Self::Null => Ok(()),
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

async fn dispatch_http(url: &str, events: &[TrackEventRequest]) -> Result<(), CaptureError> {
    let client = reqwest::Client::new();
    let body = serde_json::to_value(events)
        .map_err(|e| CaptureError::SerializeError(e.to_string()))?;

    client.post(url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| CaptureError::HttpError(e.to_string()))?;

    Ok(())
}

async fn dispatch_file(path: &PathBuf, events: &[TrackEventRequest]) -> Result<(), CaptureError> {
    use tokio::io::AsyncWriteExt;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| CaptureError::IoError(e.to_string()))?;
    }

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .map_err(|e| CaptureError::IoError(e.to_string()))?;

    for event in events {
        let line = serde_json::to_string(event)
            .map_err(|e| CaptureError::SerializeError(e.to_string()))?;
        file.write_all(line.as_bytes()).await
            .map_err(|e| CaptureError::IoError(e.to_string()))?;
        file.write_all(b"\n").await
            .map_err(|e| CaptureError::IoError(e.to_string()))?;
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("I/O error: {0}")]
    IoError(String),
    #[error("Serialization error: {0}")]
    SerializeError(String),
}
