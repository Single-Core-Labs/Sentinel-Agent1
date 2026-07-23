use async_trait::async_trait;
use sentinel_protocol::Message;

#[async_trait]
pub trait ContentCompressor: Send + Sync {
    fn name(&self) -> &'static str;
    async fn compress(&self, tool_name: &str, output: &str, is_error: bool) -> String;
    async fn compress_conversation(&self, messages: &[Message], model: &str) -> Vec<Message>;
}

pub struct NullCompressor;

impl NullCompressor {
    pub fn new() -> Self { Self }
}

impl Default for NullCompressor {
    fn default() -> Self { Self }
}

#[async_trait]
impl ContentCompressor for NullCompressor {
    fn name(&self) -> &'static str { "null" }
    async fn compress(&self, _tool_name: &str, output: &str, _is_error: bool) -> String {
        output.to_string()
    }
    async fn compress_conversation(&self, messages: &[Message], _model: &str) -> Vec<Message> {
        messages.to_vec()
    }
}
