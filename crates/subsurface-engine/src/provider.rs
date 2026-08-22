use std::process::Command;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProviderError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Rate limited: {0}")]
    RateLimit(String),
    #[error("Provider error: {0}")]
    Other(String),
}

/// The single trait for inference providers.
pub trait Provider: Send + Sync {
    fn complete(&self, prompt: &str) -> Result<String, ProviderError>;
}

/// A deterministic, offline test fake that returns a canned response.
#[derive(Debug, Clone, Default)]
pub struct FakeProvider {
    canned_response: String,
}

impl FakeProvider {
    pub fn new(canned_response: impl Into<String>) -> Self {
        Self {
            canned_response: canned_response.into(),
        }
    }
}

impl Provider for FakeProvider {
    fn complete(&self, _prompt: &str) -> Result<String, ProviderError> {
        Ok(self.canned_response.clone())
    }
}

/// Presets for OpenAI-compatible providers.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProviderPreset {
    pub name: &'static str,
    pub default_base_url: &'static str,
    pub suggested_models: &'static [&'static str],
}

pub const PRESET_OPENAI: ProviderPreset = ProviderPreset {
    name: "OpenAI",
    default_base_url: "https://api.openai.com/v1",
    suggested_models: &["gpt-4o", "gpt-4o-mini", "o1", "o3-mini"],
};

pub const PRESET_GROK: ProviderPreset = ProviderPreset {
    name: "Grok",
    default_base_url: "https://api.x.ai/v1",
    suggested_models: &["grok-2", "grok-2-mini"],
};

pub const PRESET_OPENROUTER: ProviderPreset = ProviderPreset {
    name: "OpenRouter",
    default_base_url: "https://openrouter.ai/api/v1",
    suggested_models: &[
        "anthropic/claude-3.5-sonnet",
        "google/gemini-flash-1.5",
        "deepseek/deepseek-chat",
    ],
};

pub const PRESET_OPENCODE_ZEN: ProviderPreset = ProviderPreset {
    name: "OpenCode Zen",
    default_base_url: "https://api.opencodezen.com/v1",
    suggested_models: &["zen-1"],
};

pub const PRESET_OLLAMA: ProviderPreset = ProviderPreset {
    name: "Ollama (Local)",
    default_base_url: "http://localhost:11434/v1",
    suggested_models: &["llama3.2", "qwen2.5-coder", "mistral"],
};

pub const ALL_PRESETS: &[ProviderPreset] = &[
    PRESET_OPENAI,
    PRESET_GROK,
    PRESET_OPENROUTER,
    PRESET_OPENCODE_ZEN,
    PRESET_OLLAMA,
];

/// Keychain helper for macOS storing keys securely in the OS keychain.
pub struct KeychainStore;

impl KeychainStore {
    pub fn save_key(service: &str, key: &str) -> Result<(), String> {
        let status = Command::new("security")
            .args([
                "add-generic-password",
                "-a",
                "subsurface",
                "-s",
                service,
                "-w",
                key,
                "-U",
            ])
            .status()
            .map_err(|e| e.to_string())?;

        if status.success() {
            Ok(())
        } else {
            Err("Failed to save key in OS keychain".into())
        }
    }

    pub fn get_key(service: &str) -> Result<Option<String>, String> {
        let output = Command::new("security")
            .args([
                "find-generic-password",
                "-a",
                "subsurface",
                "-s",
                service,
                "-w",
            ])
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(Some(key))
        } else {
            Ok(None)
        }
    }

    pub fn delete_key(service: &str) -> Result<(), String> {
        let _ = Command::new("security")
            .args([
                "delete-generic-password",
                "-a",
                "subsurface",
                "-s",
                service,
            ])
            .output();
        Ok(())
    }
}

/// Generic OpenAI-compatible HTTP inference provider.
#[derive(Debug, Clone)]
pub struct OpenAICompatibleProvider {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl OpenAICompatibleProvider {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            model: model.into(),
        }
    }
}

#[derive(Serialize)]
struct ChatCompletionMessage {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatCompletionMessage>,
}

#[derive(serde::Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionMessageResp,
}

#[derive(serde::Deserialize)]
struct ChatCompletionMessageResp {
    content: Option<String>,
}

#[derive(serde::Deserialize)]
struct ChatCompletionResponse {
    choices: Option<Vec<ChatCompletionChoice>>,
}

impl Provider for OpenAICompatibleProvider {
    fn complete(&self, prompt: &str) -> Result<String, ProviderError> {
        let endpoint = format!("{}/chat/completions", self.base_url);
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(45))
            .build()
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let request_body = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![ChatCompletionMessage {
                role: "user",
                content: prompt.to_string(),
            }],
        };

        let mut req = client.post(&endpoint).json(&request_body);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }

        let resp = req.send().map_err(|e| ProviderError::Network(e.to_string()))?;
        let status = resp.status();

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::Auth("API key was rejected by provider (401/403)".into()));
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimit("Rate limit exceeded from provider (429)".into()));
        }

        if !status.is_success() {
            let error_text = resp.text().unwrap_or_default();
            return Err(ProviderError::Other(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let body: ChatCompletionResponse = resp
            .json()
            .map_err(|e| ProviderError::Other(format!("Failed to parse response JSON: {}", e)))?;

        let content = body
            .choices
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.message.content)
            .unwrap_or_default();

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presets_exist() {
        assert_eq!(ALL_PRESETS.len(), 5);
        assert_eq!(PRESET_OPENAI.default_base_url, "https://api.openai.com/v1");
        assert_eq!(PRESET_OLLAMA.default_base_url, "http://localhost:11434/v1");
    }
}
