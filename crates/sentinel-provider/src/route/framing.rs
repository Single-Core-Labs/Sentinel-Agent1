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
pub fn sse_frame_stream(
    response: reqwest::Response,
) -> FrameStream {
    use futures::StreamExt;

    let stream = response.bytes_stream().map(|chunk| {
        chunk
            .map(|b| b.to_vec())
            .map_err(|e| ProviderError::StreamError(e.to_string()))
    });

    let parsed = stream.flat_map(|chunk_result| {
        let bytes = match chunk_result {
            Ok(b) => b,
            Err(e) => return futures::stream::once(async { Err(e) }).boxed(),
        };
        let text = String::from_utf8_lossy(&bytes);
        let mut frames = Vec::new();
        for line in text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    continue;
                }
                frames.push(Ok(data.as_bytes().to_vec()));
            }
        }
        futures::stream::iter(frames).boxed()
    });

    Box::new(parsed)
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
