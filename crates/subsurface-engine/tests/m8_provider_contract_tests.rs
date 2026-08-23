mod support;

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use subsurface_engine::provider::{
    KeychainStore, NativeProvider, OpenCodeBridge, Provider, ProviderConnectionPreferences,
    ProviderError, ProviderKind, ProviderProtocol,
};
use subsurface_engine::store::SqliteStore;
use support::{LocalHttpFake, StubResponse};

#[test]
fn local_http_fake_starts_and_stops() {
    let address;
    {
        let server = LocalHttpFake::start();
        address = server.address();
        for _ in 0..2 {
            let mut stream = TcpStream::connect(address).expect("connect to local HTTP fake");
            stream
                .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .expect("request local HTTP fake");
            let mut response = String::new();
            stream
                .read_to_string(&mut response)
                .expect("read local HTTP fake response");
            assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
            assert!(response.contains("\r\nContent-Length: 11\r\n"));
            assert!(response.ends_with("{\"ok\":true}"));
        }
    }

    assert!(TcpStream::connect(address).is_err());
}

#[test]
fn per_connection_keys_do_not_overwrite() {
    let first = ProviderConnectionPreferences {
        id: "openai-primary".into(),
        name: "OpenAI Primary".into(),
        provider: ProviderKind::OpenAi,
        base_url: "https://api.openai.com/v1".into(),
        model: "gpt-5".into(),
        protocol: ProviderProtocol::Responses,
        is_local: false,
    };
    let second = ProviderConnectionPreferences {
        id: "open-code-go".into(),
        name: "OpenCode Go".into(),
        provider: ProviderKind::OpenCodeGo,
        base_url: "https://opencode.ai/zen/go/v1".into(),
        model: "open-model".into(),
        protocol: ProviderProtocol::ChatCompletions,
        is_local: false,
    };

    let first_service = KeychainStore::connection_key_service(&first.id).expect("first service");
    let second_service = KeychainStore::connection_key_service(&second.id).expect("second service");
    let mut keychain = HashMap::new();
    keychain.insert(first_service.clone(), "first-secret");
    keychain.insert(second_service.clone(), "second-secret");

    assert_ne!(first_service, second_service);
    assert_eq!(keychain[&first_service], "first-secret");
    assert_eq!(keychain[&second_service], "second-secret");

    let temp = tempfile::tempdir().expect("tempdir");
    let database = temp.path().join("connections.db");
    let store = SqliteStore::open(&database).expect("store");
    store.save_provider_connection(&first).expect("save first");
    store
        .save_provider_connection(&second)
        .expect("save second");
    let mut updated_first = first.clone();
    updated_first.model = "gpt-5.1".into();
    store
        .save_provider_connection(&updated_first)
        .expect("update first");
    drop(store);

    let persisted = SqliteStore::open(&database)
        .expect("reopen store")
        .list_provider_connections()
        .expect("list connections");
    assert_eq!(persisted, vec![updated_first, second]);
    let persisted_json = serde_json::to_string(&persisted).expect("serialize preferences");
    assert!(!persisted_json.contains("first-secret"));
    assert!(!persisted_json.contains("second-secret"));
    assert!(KeychainStore::connection_key_service("  ").is_err());
}

#[test]
fn native_provider_supports_all_text_protocols() {
    let cases = [
        (
            ProviderProtocol::ChatCompletions,
            "/chat/completions",
            r#"{"choices":[{"message":{"content":"chat answer"}}]}"#,
            "chat answer",
        ),
        (
            ProviderProtocol::Responses,
            "/responses",
            r#"{"output":[{"content":[{"type":"output_text","text":"responses answer"}]}]}"#,
            "responses answer",
        ),
        (
            ProviderProtocol::Messages,
            "/messages",
            r#"{"content":[{"type":"text","text":"messages answer"}]}"#,
            "messages answer",
        ),
    ];

    for (protocol, path, response, expected) in cases {
        let server = LocalHttpFake::start_with(vec![StubResponse::json(200, response)]);
        let provider =
            NativeProvider::new(connection(server.address(), protocol), "connection-secret");

        assert_eq!(
            provider
                .complete("Inspect this change")
                .expect("completion"),
            expected
        );
        let request = server.requests().pop().expect("captured request");
        assert!(
            request.starts_with(&format!("POST {path} HTTP/1.1\r\n")),
            "unexpected request: {request:?}"
        );
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer connection-secret\r\n"));
        if protocol == ProviderProtocol::Messages {
            assert!(request
                .to_ascii_lowercase()
                .contains("x-api-key: connection-secret\r\n"));
            assert!(request.contains("anthropic-version: 2023-06-01\r\n"));
        }
        let payload: serde_json::Value =
            serde_json::from_str(request.split_once("\r\n\r\n").expect("request body").1)
                .expect("JSON payload");
        assert_eq!(payload["model"], "contract-model");
        match protocol {
            ProviderProtocol::Responses => assert_eq!(payload["input"], "Inspect this change"),
            ProviderProtocol::ChatCompletions | ProviderProtocol::Messages => {
                assert_eq!(payload["messages"][0]["content"], "Inspect this change")
            }
        }
    }
}

#[test]
fn native_provider_distinguishes_failures() {
    let server = LocalHttpFake::start_with(vec![
        StubResponse::json(401, r#"{"error":"bad key"}"#),
        StubResponse::json(429, r#"{"error":"slow down"}"#),
        StubResponse::json(200, "not json"),
        StubResponse::json(200, r#"{"choices":[{"message":{"content":"late"}}]}"#)
            .delayed(Duration::from_millis(100)),
    ]);
    let provider = NativeProvider::new(
        connection(server.address(), ProviderProtocol::ChatCompletions),
        "key",
    )
    .with_timeout(Duration::from_millis(20));

    assert!(matches!(
        provider.complete("one"),
        Err(ProviderError::Auth(_))
    ));
    assert!(matches!(
        provider.complete("two"),
        Err(ProviderError::RateLimit(_))
    ));
    assert!(matches!(
        provider.complete("three"),
        Err(ProviderError::MalformedResponse(_))
    ));
    assert!(matches!(
        provider.complete("four"),
        Err(ProviderError::Timeout(_))
    ));
}

#[test]
fn model_discovery_failure_keeps_model_field_editable() {
    let server = LocalHttpFake::start_with(vec![StubResponse::json(
        500,
        r#"{"error":"catalog unavailable"}"#,
    )]);
    let provider = NativeProvider::new(
        connection(server.address(), ProviderProtocol::Responses),
        "key",
    );

    let discovery = provider.discover_models("manually-entered-model");

    assert!(discovery.models.is_empty());
    assert_eq!(discovery.selected_model, "manually-entered-model");
    assert!(discovery.model_field_editable);
    assert!(discovery
        .error
        .expect("actionable error")
        .contains("HTTP 500"));
}

#[test]
fn model_discovery_and_optional_opencode_bridge_use_live_catalogs() {
    let server = LocalHttpFake::start_with(vec![StubResponse::json(
        200,
        r#"{"data":[{"id":"model-b"},{"id":"model-a"},{"id":"model-a"}]}"#,
    )]);
    let provider = NativeProvider::new(
        connection(server.address(), ProviderProtocol::Responses),
        "key",
    );
    let discovery = provider.discover_models("manual-model-not-in-catalog");
    assert_eq!(discovery.models, vec!["model-a", "model-b"]);
    assert_eq!(discovery.selected_model, "manual-model-not-in-catalog");
    assert!(discovery.error.is_none());

    let temp = tempfile::tempdir().expect("tempdir");
    let executable = temp.path().join("opencode");
    fs::write(
        &executable,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 1.2.3; else printf 'opencode/model-b\\nopencode/model-a\\n'; fi\n",
    )
    .expect("write OpenCode fake");
    let mut permissions = fs::metadata(&executable).expect("metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&executable, permissions).expect("make OpenCode fake executable");

    let bridge = OpenCodeBridge::from_executable(&executable).expect("detect OpenCode");
    assert_eq!(bridge.version, "1.2.3");
    assert_eq!(
        bridge
            .discover_models(Some("opencode"))
            .expect("bridge models"),
        vec!["opencode/model-a", "opencode/model-b"]
    );
}

fn connection(
    address: std::net::SocketAddr,
    protocol: ProviderProtocol,
) -> ProviderConnectionPreferences {
    ProviderConnectionPreferences {
        id: "contract".into(),
        name: "Contract".into(),
        provider: ProviderKind::OpenCodeGo,
        base_url: format!("http://{address}"),
        model: "contract-model".into(),
        protocol,
        is_local: true,
    }
}
