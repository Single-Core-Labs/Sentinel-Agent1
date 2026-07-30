/// Framing: bytes-to-frames parsing for streaming responses.
use async_trait::async_trait;
use tokio_stream::Stream;
use crate::error::ProviderError;

pub type FrameStream = Box<dyn Stream<Item = Result<Vec<u8>, ProviderError>> + Send + Unpin>;

#[async_trait]
pub trait FramingProvider: Send + Sync {
    async fn stream_frames(
        &self,
        response: reqwest::Response,
    ) -> Result<FrameStream, ProviderError>;
}

/// SSE (Server-Sent Events) framing parser.
/// Strips `data: ` prefix lines and yields each as raw bytes.
/// Buffers partial events across chunk boundaries.
pub fn sse_frame_stream(
    response: reqwest::Response,
) -> FrameStream {
    use futures::StreamExt;

    let (tx, rx) = futures::channel::mpsc::unbounded();

    tokio::spawn(async move {
        let mut buffer = Vec::<u8>::new();
        let mut byte_stream = response.bytes_stream().map(|chunk| {
            chunk
                .map(|b| b.to_vec())
                .map_err(|e| ProviderError::StreamError(e.to_string()))
        });

        while let Some(chunk_result) = byte_stream.next().await {
            let bytes = match chunk_result {
                Ok(b) => b,
                Err(e) => { let _ = tx.unbounded_send(Err(e)); return; }
            };
            buffer.extend_from_slice(&bytes);
            while let Some(pos) = buffer.windows(2).position(|w| w == b"\n\n") {
                let event_bytes = buffer[..pos].to_vec();
                buffer.drain(..pos + 2);
                let text = String::from_utf8_lossy(&event_bytes);
                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            continue;
                        }
                        if tx.unbounded_send(Ok(data.as_bytes().to_vec())).is_err() { return; }
                    }
                }
            }
        }
    });

    Box::new(rx)
}

pub struct NullFraming;

#[async_trait]
impl FramingProvider for NullFraming {
    async fn stream_frames(
        &self,
        _response: reqwest::Response,
    ) -> Result<FrameStream, ProviderError> {
        Ok(Box::new(tokio_stream::empty()))
    }
}
