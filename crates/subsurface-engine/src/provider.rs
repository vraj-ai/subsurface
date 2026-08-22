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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fake_provider_returns_canned_response() {
        let fake = FakeProvider::new("Rationale found in commit abc1234.");
        let result = fake.complete("Why does this code exist?").unwrap();
        assert_eq!(result, "Rationale found in commit abc1234.");
    }
}
