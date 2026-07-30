use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    pub enabled: bool,
    pub bot_token: String,
    pub channel: String,
    pub username: Option<String>,
    pub icon_emoji: Option<String>,
    pub auto_event_types: Vec<String>,
}

impl Default for SlackConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bot_token: String::new(),
            channel: String::new(),
            username: None,
            icon_emoji: None,
            auto_event_types: vec!["approval_required".into(), "error".into(), "turn_complete".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRequest {
    pub destination: String,
    pub title: Option<String>,
    pub message: String,
    pub severity: String,
    pub metadata: HashMap<String, String>,
    pub event_type: Option<String>,
}

impl NotificationRequest {
    pub fn new(destination: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            destination: destination.into(),
            title: None,
            message: message.into(),
            severity: "info".into(),
            metadata: HashMap::new(),
            event_type: None,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_severity(mut self, severity: impl Into<String>) -> Self {
        self.severity = severity.into();
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn error(destination: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(destination, message).with_severity("error")
    }

    pub fn warning(destination: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(destination, message).with_severity("warning")
    }

    pub fn success(destination: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(destination, message).with_severity("success")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationResult {
    pub destination: String,
    pub ok: bool,
    pub error: Option<String>,
}

pub trait NotificationProvider: Send + Sync {
    fn send<'a>(&'a self, request: &'a NotificationRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = NotificationResult> + Send + 'a>>;
}

pub struct SlackProvider {
    config: SlackConfig,
    client: reqwest::Client,
}

impl SlackProvider {
    pub fn new(config: SlackConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("reqwest client");
        Self { config, client }
    }

    fn format_text(&self, request: &NotificationRequest) -> String {
        let severity_prefix = match request.severity.as_str() {
            "error" => "[ERROR]",
            "warning" => "[WARNING]",
            "success" => "[SUCCESS]",
            _ => "[INFO]",
        };

        let mut lines = Vec::new();
        if let Some(ref title) = request.title {
            lines.push(format!("{} {}", severity_prefix, title));
        } else {
            lines.push(severity_prefix.to_string());
        }
        lines.push(request.message.clone());
        for (key, value) in &request.metadata {
            lines.push(format!("{}: {}", key, value));
        }

        let text = lines.join("\n");
        self.markdown_to_mrkdwn(&text)
    }

    fn markdown_to_mrkdwn(&self, content: &str) -> String {
        if content.is_empty() {
            return content.to_string();
        }
        let mut text = content.to_string();
        text = text.replace("&", "&amp;")
            .replace("<", "&lt;")
            .replace(">", "&gt;");

        let severity_icons = [
            ("[ERROR]", ":x:"),
            ("[WARNING]", ":warning:"),
            ("[SUCCESS]", ":white_check_mark:"),
            ("[INFO]", ":information_source:"),
        ];
        for (from, to) in &severity_icons {
            text = text.replace(from, to);
        }
        text
    }

    fn is_auto_event(&self, event_type: &str) -> bool {
        self.config.auto_event_types.iter().any(|t| t == event_type)
    }
}

impl NotificationProvider for SlackProvider {
    fn send<'a>(&'a self, request: &'a NotificationRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = NotificationResult> + Send + 'a>> {
        Box::pin(async move {
            if !self.config.enabled {
                return NotificationResult {
                    destination: request.destination.clone(),
                    ok: false,
                    error: Some("Slack notifications disabled".into()),
                };
            }

            if let Some(ref event_type) = request.event_type {
                if !self.is_auto_event(event_type) {
                    return NotificationResult {
                        destination: request.destination.clone(),
                        ok: true,
                        error: None,
                    };
                }
            }

            let payload = serde_json::json!({
                "channel": self.config.channel,
                "text": self.format_text(request),
                "mrkdwn": true,
                "unfurl_links": false,
                "unfurl_media": false,
            });

            let response = match self.client
                .post("https://slack.com/api/chat.postMessage")
                .header("Authorization", format!("Bearer {}", self.config.bot_token))
                .header("Content-Type", "application/json; charset=utf-8")
                .json(&payload)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    return NotificationResult {
                        destination: request.destination.clone(),
                        ok: false,
                        error: Some(format!("HTTP request failed: {}", e)),
                    };
                }
            };

            let status = response.status();
            let body = match response.text().await {
                Ok(b) => b,
                Err(_) => {
                    return NotificationResult {
                        destination: request.destination.clone(),
                        ok: false,
                        error: Some("Failed to read response body".into()),
                    };
                }
            };

            if !status.is_success() {
                return NotificationResult {
                    destination: request.destination.clone(),
                    ok: false,
                    error: Some(format!("Slack API returned {}", status)),
                };
            }

            let data: serde_json::Value = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(_) => {
                    return NotificationResult {
                        destination: request.destination.clone(),
                        ok: false,
                        error: Some("Invalid JSON from Slack API".into()),
                    };
                }
            };

            if data.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                NotificationResult {
                    destination: request.destination.clone(),
                    ok: true,
                    error: None,
                }
            } else {
                let error = data.get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown_error");
                NotificationResult {
                    destination: request.destination.clone(),
                    ok: false,
                    error: Some(error.to_string()),
                }
            }
        })
    }
}

pub struct NotificationGateway {
    slack: Option<SlackProvider>,
}

impl NotificationGateway {
    pub fn new() -> Self {
        Self { slack: None }
    }

    pub fn with_slack(mut self, config: SlackConfig) -> Self {
        if config.enabled && !config.bot_token.is_empty() && !config.channel.is_empty() {
            self.slack = Some(SlackProvider::new(config));
        }
        self
    }

    pub async fn send(&self, request: &NotificationRequest) -> NotificationResult {
        if let Some(ref slack) = self.slack {
            slack.send(request).await
        } else {
            NotificationResult {
                destination: request.destination.clone(),
                ok: false,
                error: Some("No notification providers configured".into()),
            }
        }
    }

    pub async fn send_event(&self, _event_type: &str, title: &str, message: &str, severity: &str) -> NotificationResult {
        let req = NotificationRequest::new("slack.default", message)
            .with_title(title)
            .with_severity(severity);
        self.send(&req).await
    }

    pub fn is_configured(&self) -> bool {
        self.slack.is_some()
    }
}

impl Default for NotificationGateway {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for NotificationGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationGateway")
            .field("slack_configured", &self.slack.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_request_builder() {
        let req = NotificationRequest::new("slack.default", "Test message")
            .with_title("Test")
            .with_severity("error")
            .with_metadata("key", "value");
        assert_eq!(req.destination, "slack.default");
        assert_eq!(req.title.unwrap(), "Test");
        assert_eq!(req.severity, "error");
        assert_eq!(req.metadata.get("key").unwrap(), "value");
    }

    #[test]
    fn test_slack_config_default() {
        let config = SlackConfig::default();
        assert!(!config.enabled);
        assert!(config.bot_token.is_empty());
    }

    #[test]
    fn test_gateway_not_configured() {
        let gateway = NotificationGateway::new();
        assert!(!gateway.is_configured());
    }

    #[tokio::test]
    async fn test_gateway_disabled_returns_error() {
        let gateway = NotificationGateway::new();
        let result = gateway.send(&NotificationRequest::new("slack.default", "test")).await;
        assert!(!result.ok);
        assert!(result.error.is_some());
    }
}