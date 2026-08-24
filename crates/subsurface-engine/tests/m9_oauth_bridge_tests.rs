mod support;

use std::time::Duration;

use subsurface_engine::oauth::{DevicePollResult, OAuthClient, OAuthError};
use subsurface_engine::provider::{
    ConsentDecision, NativeProvider, OpenAICompatibleProvider, OutboundPolicy,
    ProviderConnectionPreferences, ProviderError, ProviderKind, ProviderProtocol,
};
use support::{LocalHttpFake, StubResponse};

#[test]
fn authorization_callback_refresh_and_cancel_are_complete() {
    let server = LocalHttpFake::start_with(vec![
        StubResponse::json(
            200,
            r#"{"access_token":"access-one","refresh_token":"refresh-one","expires_in":3600,"token_type":"Bearer"}"#,
        ),
        StubResponse::json(
            200,
            r#"{"access_token":"access-two","expires_in":7200,"token_type":"Bearer"}"#,
        ),
    ]);
    let base = format!("http://{}", server.address());
    let client = OAuthClient::new(
        "desktop-client",
        format!("{base}/authorize"),
        format!("{base}/token"),
    )
    .with_timeout(Duration::from_secs(1));
    let mut authorization = client
        .start_authorization(
            "http://127.0.0.1:8787/callback",
            &["models.read", "profile"],
        )
        .expect("start authorization");

    assert!(authorization
        .authorization_url
        .contains("code_challenge_method=S256"));
    assert!(authorization
        .authorization_url
        .contains("scope=models.read+profile"));
    let callback = format!(
        "http://127.0.0.1:8787/callback?code=approved-code&state={}",
        authorization.state
    );
    let tokens = client
        .exchange_callback(&mut authorization, &callback)
        .expect("exchange callback");
    assert_eq!(tokens.access_token, "access-one");
    assert_eq!(tokens.refresh_token.as_deref(), Some("refresh-one"));
    assert!(!tokens.is_expired());

    let refreshed = client
        .refresh_tokens(tokens.refresh_token.as_deref().unwrap())
        .expect("refresh tokens");
    assert_eq!(refreshed.access_token, "access-two");
    assert_eq!(refreshed.refresh_token.as_deref(), Some("refresh-one"));

    let requests = server.requests();
    assert!(requests[0].starts_with("POST /token HTTP/1.1\r\n"));
    assert!(requests[0].contains("grant_type=authorization_code"));
    assert!(requests[0].contains("code_verifier="));
    assert!(requests[1].contains("grant_type=refresh_token"));

    assert!(matches!(
        client.exchange_callback(&mut authorization, &callback),
        Err(OAuthError::AlreadyCompleted)
    ));
    authorization.cancel();
    assert!(matches!(
        client.exchange_callback(&mut authorization, &callback),
        Err(OAuthError::Cancelled)
    ));
    assert_eq!(server.requests().len(), 2);
}

#[test]
fn callback_rejects_state_mismatch_before_network() {
    let client = OAuthClient::new(
        "desktop-client",
        "https://identity.example/authorize",
        "https://identity.example/token",
    );
    let mut authorization = client
        .start_authorization("http://127.0.0.1:8787/callback", &[])
        .expect("start authorization");

    assert!(matches!(
        client.exchange_callback(
            &mut authorization,
            "http://127.0.0.1:8787/callback?code=stolen&state=wrong"
        ),
        Err(OAuthError::StateMismatch)
    ));
}

#[test]
fn device_authorization_polls_to_completion_and_can_cancel() {
    let server = LocalHttpFake::start_with(vec![
        StubResponse::json(
            200,
            r#"{"device_code":"device-secret","user_code":"ABCD-EFGH","verification_uri":"https://identity.example/device","verification_uri_complete":"https://identity.example/device?user_code=ABCD-EFGH","expires_in":600,"interval":1}"#,
        ),
        StubResponse::json(400, r#"{"error":"authorization_pending"}"#),
        StubResponse::json(
            200,
            r#"{"access_token":"device-access","refresh_token":"device-refresh","expires_in":3600,"token_type":"Bearer"}"#,
        ),
    ]);
    let base = format!("http://{}", server.address());
    let client = OAuthClient::new(
        "desktop-client",
        format!("{base}/authorize"),
        format!("{base}/token"),
    )
    .with_device_authorization_url(format!("{base}/device/code"))
    .with_timeout(Duration::from_secs(1));
    let mut device = client
        .start_device_authorization(&["models.read"])
        .expect("start device flow");

    assert_eq!(device.user_code, "ABCD-EFGH");
    assert_eq!(device.interval_secs, 1);
    assert!(matches!(
        client
            .poll_device_tokens(&mut device)
            .expect("pending poll"),
        DevicePollResult::Pending {
            retry_after_secs: 1
        }
    ));
    let DevicePollResult::Complete(tokens) = client
        .poll_device_tokens(&mut device)
        .expect("complete device flow")
    else {
        panic!("device flow did not complete");
    };
    assert_eq!(tokens.access_token, "device-access");

    assert!(matches!(
        client.poll_device_tokens(&mut device),
        Err(OAuthError::AlreadyCompleted)
    ));
    device.cancel();
    assert!(matches!(
        client.poll_device_tokens(&mut device),
        Err(OAuthError::Cancelled)
    ));
    assert_eq!(server.requests().len(), 3);
}

#[test]
fn oauth_timeout_is_distinct() {
    let server = LocalHttpFake::start_with(vec![StubResponse::json(
        200,
        r#"{"access_token":"too-late"}"#,
    )
    .delayed(Duration::from_millis(100))]);
    let client = OAuthClient::new(
        "desktop-client",
        "https://identity.example/authorize",
        format!("http://{}/token", server.address()),
    )
    .with_timeout(Duration::from_millis(20));

    assert!(matches!(
        client.exchange_code_for_tokens("code", "http://127.0.0.1/callback"),
        Err(OAuthError::Timeout(_))
    ));
}

#[test]
fn oauth_secrets_are_not_forwarded_across_redirects() {
    let receiver = LocalHttpFake::start();
    let redirector = LocalHttpFake::start_with(vec![StubResponse::redirect(
        307,
        format!("http://{}/collect", receiver.address()),
    )]);
    let client = OAuthClient::new(
        "desktop-client",
        "https://identity.example/authorize",
        format!("http://{}/token", redirector.address()),
    )
    .with_timeout(Duration::from_secs(1));

    assert!(matches!(
        client.refresh_tokens("refresh-secret"),
        Err(OAuthError::ExchangeFailed(_))
    ));
    assert!(receiver.requests().is_empty());
}

#[test]
fn offline_mode_blocks_external_call() {
    let server = LocalHttpFake::start_with(vec![chat_response("should-not-arrive")]);
    let provider = provider_for(&server);
    let mut policy = OutboundPolicy::new(true);
    let project = std::path::Path::new("/tmp/example-project");

    let preview = policy.preview(&provider, project, "private source");
    assert_eq!(preview.project_path, project);
    assert_eq!(preview.payload["messages"][0]["content"], "private source");
    assert!(matches!(
        policy.complete(
            &provider,
            project,
            "private source",
            Some(ConsentDecision::AllowOnce)
        ),
        Err(ProviderError::Offline)
    ));
    assert!(server.requests().is_empty());
}

#[test]
fn consent_defaults_to_each_request_and_always_allow_is_project_scoped() {
    let server = LocalHttpFake::start_with(vec![
        chat_response("once"),
        chat_response("always"),
        chat_response("remembered"),
    ]);
    let provider = provider_for(&server);
    let mut policy = OutboundPolicy::new(false);
    let project = std::path::Path::new("/tmp/project-one");

    assert!(matches!(
        policy.complete(&provider, project, "first", None),
        Err(ProviderError::ConsentRequired)
    ));
    assert_eq!(
        policy
            .complete(
                &provider,
                project,
                "first",
                Some(ConsentDecision::AllowOnce)
            )
            .unwrap(),
        "once"
    );
    assert!(matches!(
        policy.complete(&provider, project, "second", None),
        Err(ProviderError::ConsentRequired)
    ));
    assert_eq!(
        policy
            .complete(
                &provider,
                project,
                "second",
                Some(ConsentDecision::AlwaysAllowProject)
            )
            .unwrap(),
        "always"
    );
    assert_eq!(
        policy.complete(&provider, project, "third", None).unwrap(),
        "remembered"
    );
    assert!(matches!(
        policy.complete(
            &provider,
            std::path::Path::new("/tmp/project-two"),
            "other",
            None
        ),
        Err(ProviderError::ConsentRequired)
    ));
    assert_eq!(server.requests().len(), 3);
}

#[test]
fn provider_payload_is_not_forwarded_across_redirects() {
    let receiver = LocalHttpFake::start();
    let redirector = LocalHttpFake::start_with(vec![StubResponse::redirect(
        307,
        format!("http://{}/collect", receiver.address()),
    )]);
    let provider = provider_for(&redirector);
    let mut policy = OutboundPolicy::new(false);

    assert!(policy
        .complete(
            &provider,
            std::path::Path::new("/tmp/example-project"),
            "private source",
            Some(ConsentDecision::AllowOnce)
        )
        .is_err());
    assert!(receiver.requests().is_empty());
}

#[test]
fn legacy_provider_preview_matches_the_body_it_sends() {
    let provider =
        OpenAICompatibleProvider::new("https://provider.example/v1", "secret", "selected-model");
    let policy = OutboundPolicy::new(false);

    let preview = policy.preview(
        &provider,
        std::path::Path::new("/tmp/example-project"),
        "private source",
    );
    assert_eq!(preview.payload["model"], "selected-model");
    assert_eq!(preview.payload["messages"][0]["role"], "user");
    assert_eq!(preview.payload["messages"][0]["content"], "private source");
}

fn provider_for(server: &LocalHttpFake) -> NativeProvider {
    NativeProvider::new(
        ProviderConnectionPreferences {
            id: "consent-test".into(),
            name: "Consent Test".into(),
            provider: ProviderKind::Custom,
            base_url: format!("http://{}", server.address()),
            model: "test-model".into(),
            protocol: ProviderProtocol::ChatCompletions,
            is_local: false,
        },
        "test-key",
    )
    .with_timeout(Duration::from_secs(1))
}

fn chat_response(text: &str) -> StubResponse {
    StubResponse::json(
        200,
        format!(r#"{{"choices":[{{"message":{{"content":"{text}"}}}}]}}"#),
    )
}
