/// Protocol: the semantic API contract for a provider.
/// Decomposed into Body, Frame, Event, State generics.
use async_trait::async_trait;
use sentinel_protocol::{CompletionRequest, CompletionResponse};
use crate::error::ProviderError;
use super::framing::{FrameStream, FramingProvider};

#[async_trait]
pub trait Protocol: Send + Sync {
    type Body: Send;
    type Frame: Send;
    type Event: Send;
    type State: Send;

    /// Build the request body from a CompletionRequest.
    fn build_body(&self, req: &CompletionRequest) -> Result<Self::Body, ProviderError>;

    /// Serialize the body to JSON bytes for the HTTP request.
    fn serialize_body(&self, body: &Self::Body) -> Result<Vec<u8>, ProviderError>;

    /// Parse a single frame from the stream into an event.
    fn parse_frame(&self, frame: Vec<u8>) -> Result<Option<Self::Event>, ProviderError>;

    /// Process an event and update the state machine.
    fn apply_event(&self, state: &mut Self::State, event: Self::Event);

    /// Finalize the state into a CompletionResponse.
    fn finalize(&self, state: Self::State) -> CompletionResponse;

    /// Create initial state for a new stream.
    fn initial_state(&self) -> Self::State;
}

/// Route composes a Protocol with Endpoint, Auth, and Framing.
pub struct Route<P: Protocol> {
    pub protocol: P,
    pub endpoint: super::endpoint::Endpoint,
    pub auth: super::auth::Auth,
    pub framing: Box<dyn FramingProvider>,
    pub client: reqwest::Client,
}

impl<P: Protocol> std::fmt::Debug for Route<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Route")
            .field("endpoint", &self.endpoint)
            .field("auth", &self.auth)
            .finish()
    }
}

impl<P: Protocol> Route<P> {
    pub fn new(
        protocol: P,
        endpoint: super::endpoint::Endpoint,
        auth: super::auth::Auth,
        framing: Box<dyn FramingProvider>,
    ) -> Self {
        let timeout = std::time::Duration::from_secs(120);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("valid reqwest client");
        Self { protocol, endpoint, auth, framing, client }
    }

    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    pub async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let body = self.protocol.build_body(req)?;
        let json_bytes = self.protocol.serialize_body(&body)?;
        let url = self.endpoint.chat_url();

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().expect("valid header"),
        );
        self.auth.apply(&mut headers);

        let resp = self.client
            .post(&url)
            .headers(headers)
            .body(json_bytes)
            .send()
            .await
            .map_err(|e| ProviderError::RequestError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ApiError(format!("HTTP {}: {}", status, text)));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::JsonError(e.to_string()))?;

        let response: CompletionResponse = serde_json::from_value(json)
            .map_err(|e| ProviderError::JsonError(e.to_string()))?;

        Ok(response)
    }

    pub async fn complete_stream(
        &self,
        req: &CompletionRequest,
    ) -> Result<Box<dyn tokio_stream::Stream<Item = Result<sentinel_protocol::StreamChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        let body = self.protocol.build_body(req)?;
        let json_bytes = self.protocol.serialize_body(&body)?;
        let url = self.endpoint.chat_url();

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().expect("valid header"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            "text/event-stream".parse().expect("valid header"),
        );
        self.auth.apply(&mut headers);

        let resp = self.client
            .post(&url)
            .headers(headers)
            .body(json_bytes)
            .send()
            .await
            .map_err(|e| ProviderError::RequestError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ApiError(format!("HTTP {}: {}", status, text)));
        }

        let frame_stream = self.framing.stream_frames(resp).await?;

        let parsed = tokio_stream::StreamExt::map(frame_stream, |frame_result| {
            frame_result.and_then(|bytes| {
                serde_json::from_slice::<sentinel_protocol::StreamChunk>(&bytes)
                    .map_err(|e| ProviderError::JsonError(e.to_string()))
            })
        });

        Ok(Box::new(parsed))
    }
}
