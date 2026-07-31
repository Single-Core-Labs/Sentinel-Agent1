use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Id = serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Id,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self { code: -32700, message: msg.into(), data: None }
    }
    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self { code: -32600, message: msg.into(), data: None }
    }
    pub fn method_not_found(msg: impl Into<String>) -> Self {
        Self { code: -32601, message: msg.into(), data: None }
    }
    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self { code: -32602, message: msg.into(), data: None }
    }
    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self { code: -32603, message: msg.into(), data: None }
    }
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Request(JsonRpcRequest),
    Notification(JsonRpcNotification),
    Response(JsonRpcResponse),
}

pub fn parse_message(data: &str) -> Result<JsonRpcMessage, JsonRpcError> {
    serde_json::from_str(data).map_err(|e| {
        JsonRpcError::parse_error(format!("Invalid JSON: {}", e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_follow_json_rpc_spec() {
        assert_eq!(JsonRpcError::parse_error("x").code, -32700);
        assert_eq!(JsonRpcError::invalid_request("x").code, -32600);
        assert_eq!(JsonRpcError::method_not_found("x").code, -32601);
        assert_eq!(JsonRpcError::invalid_params("x").code, -32602);
        assert_eq!(JsonRpcError::internal_error("x").code, -32603);
    }

    #[test]
    fn parse_request_with_params() {
        let data = r#"{"jsonrpc":"2.0","id":7,"method":"chat","params":{"session_id":"s1"}}"#;
        match parse_message(data).unwrap() {
            JsonRpcMessage::Request(req) => {
                assert_eq!(req.id, 7);
                assert_eq!(req.method, "chat");
                assert_eq!(req.params.as_ref().unwrap()["session_id"], "s1");
            }
            _ => panic!("expected request"),
        }
    }

    #[test]
    fn parse_notification_has_no_id() {
        let data = r#"{"jsonrpc":"2.0","method":"exit"}"#;
        match parse_message(data).unwrap() {
            JsonRpcMessage::Notification(n) => assert_eq!(n.method, "exit"),
            _ => panic!("expected notification"),
        }
    }

    #[test]
    fn parse_response_roundtrip_omits_error_when_absent() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: serde_json::json!("abc"),
            result: Some(serde_json::json!({"ok": true})),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("error"), "error must be omitted: {}", json);
        let back: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.result, resp.result);
    }

    #[test]
    fn parse_invalid_json_returns_parse_error() {
        let err = parse_message("not json").unwrap_err();
        assert_eq!(err.code, -32700);
    }

    #[test]
    fn parse_error_with_data() {
        let err = JsonRpcError::internal_error("boom").with_data(serde_json::json!({"detail": 1}));
        assert_eq!(err.data.as_ref().unwrap()["detail"], 1);
    }
}
