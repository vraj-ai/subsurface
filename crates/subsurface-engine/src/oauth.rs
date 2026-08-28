use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OAuthError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("OAuth request timed out: {0}")]
    Timeout(String),
    #[error("Token expired and no refresh token available")]
    TokenExpired,
    #[error("OAuth token exchange failed: {0}")]
    ExchangeFailed(String),
    #[error("Invalid OAuth URL: {0}")]
    InvalidUrl(String),
    #[error("OAuth callback did not match the expected redirect")]
    InvalidCallback,
    #[error("OAuth callback state did not match")]
    StateMismatch,
    #[error("OAuth authorization was denied: {0}")]
    AuthorizationDenied(String),
    #[error("OAuth flow was cancelled")]
    Cancelled,
    #[error("OAuth flow was already completed")]
    AlreadyCompleted,
    #[error("This connection does not expose device authorization")]
    DeviceUnsupported,
    #[error("Device authorization expired")]
    DeviceExpired,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_timestamp: i64,
    pub token_type: String,
}

impl fmt::Debug for OAuthTokens {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthTokens")
            .field("access_token", &"[redacted]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[redacted]"),
            )
            .field("expires_at_timestamp", &self.expires_at_timestamp)
            .field("token_type", &self.token_type)
            .finish()
    }
}

impl OAuthTokens {
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() >= self.expires_at_timestamp
    }
}

pub struct OAuthAuthorization {
    pub authorization_url: String,
    pub state: String,
    redirect_uri: String,
    code_verifier: String,
    cancelled: bool,
    completed: bool,
}

impl OAuthAuthorization {
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
}

pub struct DeviceAuthorization {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_at_timestamp: i64,
    pub interval_secs: u64,
    device_code: String,
    cancelled: bool,
    completed: bool,
}

impl DeviceAuthorization {
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum DevicePollResult {
    Pending { retry_after_secs: u64 },
    Complete(OAuthTokens),
}

pub struct OAuthClient {
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
    device_authorization_url: Option<String>,
    timeout: Duration,
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
            device_authorization_url: None,
            timeout: Duration::from_secs(45),
        }
    }

    pub fn with_device_authorization_url(mut self, url: impl Into<String>) -> Self {
        self.device_authorization_url = Some(url.into());
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn get_authorization_url(&self, redirect_uri: &str, state: &str) -> String {
        self.authorization_url(redirect_uri, state, None, &[])
            .unwrap_or_else(|_| {
                format!(
                    "{}?response_type=code&client_id={}&redirect_uri={}&state={}",
                    self.auth_url, self.client_id, redirect_uri, state
                )
            })
    }

    pub fn start_authorization(
        &self,
        redirect_uri: &str,
        scopes: &[&str],
    ) -> Result<OAuthAuthorization, OAuthError> {
        reqwest::Url::parse(redirect_uri)
            .map_err(|error| OAuthError::InvalidUrl(error.to_string()))?;
        let state = Uuid::new_v4().simple().to_string();
        let code_verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
        let authorization_url =
            self.authorization_url(redirect_uri, &state, Some(&code_challenge), scopes)?;
        Ok(OAuthAuthorization {
            authorization_url,
            state,
            redirect_uri: redirect_uri.into(),
            code_verifier,
            cancelled: false,
            completed: false,
        })
    }

    pub fn exchange_callback(
        &self,
        authorization: &mut OAuthAuthorization,
        callback_url: &str,
    ) -> Result<OAuthTokens, OAuthError> {
        if authorization.cancelled {
            return Err(OAuthError::Cancelled);
        }
        if authorization.completed {
            return Err(OAuthError::AlreadyCompleted);
        }
        let callback = reqwest::Url::parse(callback_url)
            .map_err(|error| OAuthError::InvalidUrl(error.to_string()))?;
        let expected = reqwest::Url::parse(&authorization.redirect_uri)
            .map_err(|error| OAuthError::InvalidUrl(error.to_string()))?;
        if callback.scheme() != expected.scheme()
            || callback.host_str() != expected.host_str()
            || callback.port_or_known_default() != expected.port_or_known_default()
            || callback.path() != expected.path()
        {
            return Err(OAuthError::InvalidCallback);
        }
        let values: std::collections::HashMap<_, _> = callback.query_pairs().into_owned().collect();
        if let Some(error) = values.get("error") {
            return Err(OAuthError::AuthorizationDenied(
                values
                    .get("error_description")
                    .cloned()
                    .unwrap_or_else(|| error.clone()),
            ));
        }
        if values.get("state") != Some(&authorization.state) {
            return Err(OAuthError::StateMismatch);
        }
        let code = values
            .get("code")
            .ok_or_else(|| OAuthError::ExchangeFailed("callback did not contain a code".into()))?;
        let tokens = self.exchange_code(
            code,
            &authorization.redirect_uri,
            Some(&authorization.code_verifier),
        )?;
        authorization.completed = true;
        Ok(tokens)
    }

    pub fn exchange_code_for_tokens(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<OAuthTokens, OAuthError> {
        self.exchange_code(code, redirect_uri, None)
    }

    pub fn refresh_tokens(&self, refresh_token: &str) -> Result<OAuthTokens, OAuthError> {
        self.token_request(
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", &self.client_id),
            ],
            Some(refresh_token),
        )
    }

    pub fn ensure_fresh_tokens(&self, tokens: &OAuthTokens) -> Result<OAuthTokens, OAuthError> {
        if !tokens.is_expired() {
            return Ok(tokens.clone());
        }
        let refresh_token = tokens
            .refresh_token
            .as_deref()
            .ok_or(OAuthError::TokenExpired)?;
        self.refresh_tokens(refresh_token)
    }

    pub fn start_device_authorization(
        &self,
        scopes: &[&str],
    ) -> Result<DeviceAuthorization, OAuthError> {
        let url = self
            .device_authorization_url
            .as_ref()
            .ok_or(OAuthError::DeviceUnsupported)?;
        let scope = scopes.join(" ");
        let response = self.post_form(url, &[("client_id", &self.client_id), ("scope", &scope)])?;
        if !response.status().is_success() {
            return Err(OAuthError::ExchangeFailed(format!(
                "HTTP {}",
                response.status()
            )));
        }
        #[derive(Deserialize)]
        struct DeviceResponse {
            device_code: String,
            user_code: String,
            verification_uri: String,
            verification_uri_complete: Option<String>,
            expires_in: i64,
            interval: Option<u64>,
        }
        let body: DeviceResponse = response
            .json()
            .map_err(|error| OAuthError::ExchangeFailed(error.to_string()))?;
        Ok(DeviceAuthorization {
            user_code: body.user_code,
            verification_uri: body.verification_uri,
            verification_uri_complete: body.verification_uri_complete,
            expires_at_timestamp: Utc::now().timestamp() + body.expires_in,
            interval_secs: body.interval.unwrap_or(5).max(1),
            device_code: body.device_code,
            cancelled: false,
            completed: false,
        })
    }

    pub fn poll_device_tokens(
        &self,
        authorization: &mut DeviceAuthorization,
    ) -> Result<DevicePollResult, OAuthError> {
        if authorization.cancelled {
            return Err(OAuthError::Cancelled);
        }
        if authorization.completed {
            return Err(OAuthError::AlreadyCompleted);
        }
        if Utc::now().timestamp() >= authorization.expires_at_timestamp {
            return Err(OAuthError::DeviceExpired);
        }
        let response = self.post_form(
            &self.token_url,
            &[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &authorization.device_code),
                ("client_id", &self.client_id),
            ],
        )?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|error| classify_network_error(&error))?;
        if status.is_success() {
            let tokens = parse_tokens(&body, None)?;
            authorization.completed = true;
            return Ok(DevicePollResult::Complete(tokens));
        }
        #[derive(Deserialize)]
        struct DeviceError {
            error: String,
            error_description: Option<String>,
        }
        let error: DeviceError = serde_json::from_str(&body)
            .map_err(|parse| OAuthError::ExchangeFailed(parse.to_string()))?;
        match error.error.as_str() {
            "authorization_pending" => Ok(DevicePollResult::Pending {
                retry_after_secs: authorization.interval_secs,
            }),
            "slow_down" => Ok(DevicePollResult::Pending {
                retry_after_secs: authorization.interval_secs + 5,
            }),
            "expired_token" => Err(OAuthError::DeviceExpired),
            "access_denied" => Err(OAuthError::AuthorizationDenied(
                error
                    .error_description
                    .unwrap_or_else(|| "device authorization denied".into()),
            )),
            _ => Err(OAuthError::ExchangeFailed(
                error.error_description.unwrap_or(error.error),
            )),
        }
    }

    fn authorization_url(
        &self,
        redirect_uri: &str,
        state: &str,
        code_challenge: Option<&str>,
        scopes: &[&str],
    ) -> Result<String, OAuthError> {
        let mut url = reqwest::Url::parse(&self.auth_url)
            .map_err(|error| OAuthError::InvalidUrl(error.to_string()))?;
        let scope = scopes.join(" ");
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("state", state);
        if !scope.is_empty() {
            query.append_pair("scope", &scope);
        }
        if let Some(challenge) = code_challenge {
            query
                .append_pair("code_challenge", challenge)
                .append_pair("code_challenge_method", "S256");
        }
        drop(query);
        Ok(url.into())
    }

    fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        code_verifier: Option<&str>,
    ) -> Result<OAuthTokens, OAuthError> {
        let mut form = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", redirect_uri),
        ];
        if let Some(verifier) = code_verifier {
            form.push(("code_verifier", verifier));
        }
        self.token_request(&form, None)
    }

    fn token_request(
        &self,
        form: &[(&str, &str)],
        previous_refresh_token: Option<&str>,
    ) -> Result<OAuthTokens, OAuthError> {
        let response = self.post_form(&self.token_url, form)?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|error| classify_network_error(&error))?;
        if !status.is_success() {
            return Err(OAuthError::ExchangeFailed(format!("HTTP {status}")));
        }
        parse_tokens(&body, previous_refresh_token)
    }

    fn post_form(
        &self,
        url: &str,
        form: &[(&str, &str)],
    ) -> Result<reqwest::blocking::Response, OAuthError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| OAuthError::Network(error.to_string()))?;
        client
            .post(url)
            .form(form)
            .send()
            .map_err(|error| classify_network_error(&error))
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    token_type: Option<String>,
}

fn parse_tokens(
    body: &str,
    previous_refresh_token: Option<&str>,
) -> Result<OAuthTokens, OAuthError> {
    let body: TokenResponse = serde_json::from_str(body)
        .map_err(|error| OAuthError::ExchangeFailed(error.to_string()))?;
    Ok(OAuthTokens {
        access_token: body.access_token,
        refresh_token: body
            .refresh_token
            .or_else(|| previous_refresh_token.map(str::to_owned)),
        expires_at_timestamp: Utc::now().timestamp() + body.expires_in.unwrap_or(3600),
        token_type: body.token_type.unwrap_or_else(|| "Bearer".into()),
    })
}

fn classify_network_error(error: &reqwest::Error) -> OAuthError {
    if error.is_timeout() {
        OAuthError::Timeout(error.to_string())
    } else {
        OAuthError::Network(error.to_string())
    }
}
