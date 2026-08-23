use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProviderError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Rate limited: {0}")]
    RateLimit(String),
    #[error("Provider timed out: {0}")]
    Timeout(String),
    #[error("Malformed provider response: {0}")]
    MalformedResponse(String),
    #[error("Provider error: {0}")]
    Other(String),
}

/// The single trait for inference providers.
pub trait Provider: Send + Sync {
    fn complete(&self, prompt: &str) -> Result<String, ProviderError>;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAi,
    Xai,
    OpenRouter,
    OpenCodeFree,
    OpenCodeZen,
    OpenCodeGo,
    Ollama,
    Custom,
}

impl ProviderKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "open_ai",
            Self::Xai => "xai",
            Self::OpenRouter => "open_router",
            Self::OpenCodeFree => "open_code_free",
            Self::OpenCodeZen => "open_code_zen",
            Self::OpenCodeGo => "open_code_go",
            Self::Ollama => "ollama",
            Self::Custom => "custom",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "open_ai" => Some(Self::OpenAi),
            "xai" => Some(Self::Xai),
            "open_router" => Some(Self::OpenRouter),
            "open_code_free" => Some(Self::OpenCodeFree),
            "open_code_zen" => Some(Self::OpenCodeZen),
            "open_code_go" => Some(Self::OpenCodeGo),
            "ollama" => Some(Self::Ollama),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProtocol {
    ChatCompletions,
    Responses,
    Messages,
}

impl ProviderProtocol {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ChatCompletions => "chat_completions",
            Self::Responses => "responses",
            Self::Messages => "messages",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "chat_completions" => Some(Self::ChatCompletions),
            "responses" => Some(Self::Responses),
            "messages" => Some(Self::Messages),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConnectionPreferences {
    pub id: String,
    pub name: String,
    pub provider: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub protocol: ProviderProtocol,
    pub is_local: bool,
}

#[derive(Clone)]
pub struct NativeProvider {
    pub connection: ProviderConnectionPreferences,
    api_key: String,
    timeout: Duration,
}

impl NativeProvider {
    pub fn new(connection: ProviderConnectionPreferences, api_key: impl Into<String>) -> Self {
        Self {
            connection,
            api_key: api_key.into(),
            timeout: Duration::from_secs(45),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Provider for NativeProvider {
    fn complete(&self, prompt: &str) -> Result<String, ProviderError> {
        let protocol = self.connection.protocol;
        let endpoint = format!(
            "{}/{}",
            self.connection.base_url.trim_end_matches('/'),
            match protocol {
                ProviderProtocol::ChatCompletions => "chat/completions",
                ProviderProtocol::Responses => "responses",
                ProviderProtocol::Messages => "messages",
            }
        );
        let body = match protocol {
            ProviderProtocol::ChatCompletions => json!({
                "model": self.connection.model,
                "messages": [{"role": "user", "content": prompt}],
            }),
            ProviderProtocol::Responses => json!({
                "model": self.connection.model,
                "input": prompt,
            }),
            ProviderProtocol::Messages => json!({
                "model": self.connection.model,
                "max_tokens": 2048,
                "messages": [{"role": "user", "content": prompt}],
            }),
        };
        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|error| ProviderError::Network(error.to_string()))?;
        let mut request = client.post(endpoint).json(&body);
        if !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
            if protocol == ProviderProtocol::Messages {
                request = request
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", "2023-06-01");
            }
        }
        let response = request.send().map_err(|error| {
            if error.is_timeout() {
                ProviderError::Timeout(error.to_string())
            } else {
                ProviderError::Network(error.to_string())
            }
        })?;
        let status = response.status();
        let response_body = response.text().map_err(|error| {
            if error.is_timeout() {
                ProviderError::Timeout(error.to_string())
            } else {
                ProviderError::Network(error.to_string())
            }
        })?;

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::Auth(format!("HTTP {status}")));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimit(format!("HTTP {status}")));
        }
        if !status.is_success() {
            return Err(ProviderError::Other(format!(
                "HTTP {status}: {response_body}"
            )));
        }

        let response: Value = serde_json::from_str(&response_body)
            .map_err(|error| ProviderError::MalformedResponse(error.to_string()))?;
        let text = match protocol {
            ProviderProtocol::ChatCompletions => response
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str),
            ProviderProtocol::Responses => response
                .get("output_text")
                .and_then(Value::as_str)
                .or_else(|| {
                    response
                        .pointer("/output/0/content/0/text")
                        .and_then(Value::as_str)
                }),
            ProviderProtocol::Messages => {
                response.pointer("/content/0/text").and_then(Value::as_str)
            }
        };
        text.map(str::to_owned).ok_or_else(|| {
            ProviderError::MalformedResponse("response did not contain text output".into())
        })
    }
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

pub const PRESET_XAI: ProviderPreset = ProviderPreset {
    name: "xAI",
    default_base_url: "https://api.x.ai/v1",
    suggested_models: &[],
};

pub const PRESET_GROK: ProviderPreset = PRESET_XAI;

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
    default_base_url: "https://opencode.ai/zen/v1",
    suggested_models: &[],
};

pub const PRESET_OPENCODE_FREE: ProviderPreset = ProviderPreset {
    name: "OpenCode Free",
    default_base_url: "https://opencode.ai/zen/v1",
    suggested_models: &[],
};

pub const PRESET_OPENCODE_GO: ProviderPreset = ProviderPreset {
    name: "OpenCode Go",
    default_base_url: "https://opencode.ai/zen/go/v1",
    suggested_models: &[],
};

pub const PRESET_OLLAMA: ProviderPreset = ProviderPreset {
    name: "Ollama (Local)",
    default_base_url: "http://localhost:11434/v1",
    suggested_models: &["llama3.2", "qwen2.5-coder", "mistral"],
};

pub const PRESET_CUSTOM: ProviderPreset = ProviderPreset {
    name: "Custom",
    default_base_url: "",
    suggested_models: &[],
};

pub const ALL_PRESETS: &[ProviderPreset] = &[
    PRESET_OPENAI,
    PRESET_XAI,
    PRESET_OPENROUTER,
    PRESET_OPENCODE_FREE,
    PRESET_OPENCODE_ZEN,
    PRESET_OPENCODE_GO,
    PRESET_OLLAMA,
    PRESET_CUSTOM,
];

/// Keychain helper for macOS storing keys securely in the OS keychain.
pub struct KeychainStore;

impl KeychainStore {
    pub fn connection_key_service(connection_id: &str) -> Result<String, String> {
        if connection_id.trim().is_empty() {
            return Err("Provider connection id cannot be empty".into());
        }
        Ok(format!("subsurface.provider.{connection_id}"))
    }

    pub fn save_connection_key(connection_id: &str, key: &str) -> Result<(), String> {
        Self::save_key(&Self::connection_key_service(connection_id)?, key)
    }

    pub fn get_connection_key(connection_id: &str) -> Result<Option<String>, String> {
        Self::get_key(&Self::connection_key_service(connection_id)?)
    }

    pub fn delete_connection_key(connection_id: &str) -> Result<(), String> {
        Self::delete_key(&Self::connection_key_service(connection_id)?)
    }

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
            .args(["delete-generic-password", "-a", "subsurface", "-s", service])
            .output();
        Ok(())
    }
}

/// Generic OpenAI-compatible HTTP inference provider.
#[derive(Clone)]
pub struct OpenAICompatibleProvider {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl OpenAICompatibleProvider {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            model: model.into(),
        }
    }
}

impl Provider for OpenAICompatibleProvider {
    fn complete(&self, prompt: &str) -> Result<String, ProviderError> {
        NativeProvider::new(
            ProviderConnectionPreferences {
                id: "legacy-openai-compatible".into(),
                name: "OpenAI Compatible".into(),
                provider: ProviderKind::Custom,
                base_url: self.base_url.clone(),
                model: self.model.clone(),
                protocol: ProviderProtocol::ChatCompletions,
                is_local: self.base_url.starts_with("http://localhost")
                    || self.base_url.starts_with("http://127.0.0.1"),
            },
            &self.api_key,
        )
        .complete(prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presets_exist() {
        assert_eq!(ALL_PRESETS.len(), 8);
        assert_eq!(PRESET_OPENAI.default_base_url, "https://api.openai.com/v1");
        assert_eq!(
            PRESET_OPENCODE_ZEN.default_base_url,
            "https://opencode.ai/zen/v1"
        );
        assert_eq!(
            PRESET_OPENCODE_GO.default_base_url,
            "https://opencode.ai/zen/go/v1"
        );
        assert_eq!(PRESET_OLLAMA.default_base_url, "http://localhost:11434/v1");
    }
}
