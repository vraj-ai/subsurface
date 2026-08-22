use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Token expired and no refresh token available")]
    TokenExpired,
    #[error("OAuth token exchange failed: {0}")]
    ExchangeFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_timestamp: i64,
    pub token_type: String,
}

impl OAuthTokens {
    pub fn is_expired(&self) -> bool {
        let now = Utc::now().timestamp();
        now >= self.expires_at_timestamp
    }
}

pub struct OAuthClient {
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
}

impl OAuthClient {
    pub fn new(
        client_id: impl Into<String>,
        auth_url: impl Into<String>,
        token_url: impl Into<String>,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            auth_url: auth_url.into(),
            token_url: token_url.into(),
        }
    }

    pub fn get_authorization_url(&self, redirect_uri: &str, state: &str) -> String {
        format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&state={}",
            self.auth_url, self.client_id, redirect_uri, state
        )
    }

    pub fn exchange_code_for_tokens(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<OAuthTokens, OAuthError> {
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(&self.token_url)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", &self.client_id),
                ("redirect_uri", redirect_uri),
            ])
            .send()
            .map_err(|e| OAuthError::Network(e.to_string()))?;

        #[derive(Deserialize)]
        struct TokenResp {
            access_token: String,
            refresh_token: Option<String>,
            expires_in: Option<i64>,
            token_type: Option<String>,
        }

        if resp.status().is_success() {
            let body: TokenResp = resp
                .json()
                .map_err(|e| OAuthError::ExchangeFailed(e.to_string()))?;
            let expires_at = Utc::now().timestamp() + body.expires_in.unwrap_or(3600);
            Ok(OAuthTokens {
                access_token: body.access_token,
                refresh_token: body.refresh_token,
                expires_at_timestamp: expires_at,
                token_type: body.token_type.unwrap_or_else(|| "Bearer".to_string()),
            })
        } else {
            Err(OAuthError::ExchangeFailed(format!(
                "HTTP {}",
                resp.status()
            )))
        }
    }

    pub fn refresh_tokens(&self, refresh_token: &str) -> Result<OAuthTokens, OAuthError> {
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(&self.token_url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", &self.client_id),
            ])
            .send()
            .map_err(|e| OAuthError::Network(e.to_string()))?;

        #[derive(Deserialize)]
        struct TokenResp {
            access_token: String,
            refresh_token: Option<String>,
            expires_in: Option<i64>,
            token_type: Option<String>,
        }

        if resp.status().is_success() {
            let body: TokenResp = resp
                .json()
                .map_err(|e| OAuthError::ExchangeFailed(e.to_string()))?;
            let expires_at = Utc::now().timestamp() + body.expires_in.unwrap_or(3600);
            Ok(OAuthTokens {
                access_token: body.access_token,
                refresh_token: body.refresh_token.or_else(|| Some(refresh_token.to_string())),
                expires_at_timestamp: expires_at,
                token_type: body.token_type.unwrap_or_else(|| "Bearer".to_string()),
            })
        } else {
            Err(OAuthError::ExchangeFailed(format!(
                "HTTP {}",
                resp.status()
            )))
        }
    }
}
