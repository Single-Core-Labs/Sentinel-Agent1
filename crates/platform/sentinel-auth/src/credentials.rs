use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum AuthEntry {
    #[serde(rename = "bearer")]
    Bearer { token: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Credentials {
    #[serde(flatten)]
    entries: BTreeMap<String, AuthEntry>,
}

impl Credentials {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn get(&self, provider_id: &str) -> Option<AuthEntry> {
        self.entries.get(provider_id).cloned()
    }

    pub fn set(&mut self, provider_id: String, entry: AuthEntry) {
        self.entries.insert(provider_id, entry);
    }

    pub fn remove(&mut self, provider_id: &str) -> bool {
        self.entries.remove(provider_id).is_some()
    }

    pub fn all(&self) -> Vec<(String, AuthEntry)> {
        self.entries
            .iter()
            .map(|(id, entry)| (id.clone(), entry.clone()))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get() {
        let mut creds = Credentials::new();
        let entry = AuthEntry::Bearer {
            token: "sk-test-123".to_string(),
        };
        creds.set("anthropic".to_string(), entry.clone());
        assert_eq!(creds.get("anthropic"), Some(entry));
    }

    #[test]
    fn test_remove() {
        let mut creds = Credentials::new();
        creds.set(
            "openai".to_string(),
            AuthEntry::Bearer {
                token: "sk-openai".to_string(),
            },
        );
        assert!(creds.remove("openai"));
        assert_eq!(creds.get("openai"), None);
        assert!(!creds.remove("openai")); // Already gone
    }

    #[test]
    fn test_all_lists_all_providers() {
        let mut creds = Credentials::new();
        creds.set(
            "anthropic".to_string(),
            AuthEntry::Bearer {
                token: "sk-ant".to_string(),
            },
        );
        creds.set(
            "openai".to_string(),
            AuthEntry::Bearer {
                token: "sk-openai".to_string(),
            },
        );
        assert_eq!(creds.all().len(), 2);
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut creds = Credentials::new();
        creds.set(
            "anthropic".to_string(),
            AuthEntry::Bearer {
                token: "test-token".to_string(),
            },
        );
        let json = serde_json::to_string(&creds).unwrap();
        let deserialized: Credentials = serde_json::from_str(&json).unwrap();
        assert_eq!(creds, deserialized);
    }
}
