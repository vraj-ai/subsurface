mod support;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;

use support::LocalHttpFake;
use subsurface_engine::provider::{
    KeychainStore, ProviderConnectionPreferences, ProviderKind, ProviderProtocol,
};
use subsurface_engine::store::SqliteStore;

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
    store.save_provider_connection(&second).expect("save second");
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
