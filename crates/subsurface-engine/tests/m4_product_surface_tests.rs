use std::sync::Arc;
use subsurface_engine::evidence::LineRange;
use subsurface_engine::excavate::excavate;
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::mcp::{McpServer, McpToolCall};
use subsurface_engine::provider::FakeProvider;
use subsurface_engine::site::Site;
use subsurface_engine::staleness::{detect_staleness, StalenessStatus};
use subsurface_engine::store::SqliteStore;

#[test]
fn test_staleness_flag_with_receipt() {
    let mut fixture = GitFixture::new();
    fixture.commit(
        "Workaround for bug in libv1: retry limit increased (Issue #42)",
        &[("src/client.rs", "fn send() { let retries = 5; }\n")],
    );

    // Commit that closes issue #42
    let c2 = fixture.commit(
        "Closes #42: upgrade to libv2 which fixes connection resets",
        &[("Cargo.toml", "# upgraded lib to v2.0\n")],
    );

    let site = Site::open(fixture.path()).expect("open site");
    let range = LineRange { start: 1, end: 1 };
    let provider = Arc::new(FakeProvider::new("Workaround explanation"));
    let finding = excavate(&site, "src/client.rs", range, provider).expect("excavate");

    let staleness = detect_staleness(&site, &finding);
    match staleness {
        StalenessStatus::Stale { receipt } => {
            assert!(receipt.receipt_text.contains("#42") || receipt.receipt_text.contains("Closes"));
            assert_eq!(receipt.commit_sha, c2);
        }
        StalenessStatus::Active => panic!("Expected Stale with receipt for closed issue #42"),
    }
}

#[test]
fn test_staleness_no_flag_on_old_churny_todo_code() {
    let mut fixture = GitFixture::new();
    fixture.commit(
        "TODO: this is an old workaround, clean it up someday",
        &[("src/legacy.rs", "fn legacy_guard() { /* todo */ }\n")],
    );

    // Add 10 churn commits on other lines with no receipt
    for i in 1..=10 {
        fixture.commit(
            &format!("churn commit {}", i),
            &[("src/other.rs", &format!("let v = {};\n", i))],
        );
    }

    let site = Site::open(fixture.path()).expect("open site");
    let range = LineRange { start: 1, end: 1 };
    let provider = Arc::new(FakeProvider::new("Legacy guard explanation"));
    let finding = excavate(&site, "src/legacy.rs", range, provider).expect("excavate");

    let staleness = detect_staleness(&site, &finding);
    // Invariant: Age, churn, and TODOs are NOT receipts -> must remain Active (no staleness flag)
    assert_eq!(staleness, StalenessStatus::Active);
}

#[test]
fn test_mcp_excavate_returns_structured_finding_and_records_field_note() {
    let store = Arc::new(SqliteStore::in_memory().expect("store"));
    let provider = Arc::new(FakeProvider::new("Explanation for agent"));
    let mcp = McpServer::new(store.clone(), provider);

    let mut fixture = GitFixture::new();
    fixture.commit(
        "Add calculate logic with tests",
        &[
            ("src/calc.rs", "fn calculate() -> i32 { 100 }\n"),
            ("tests/calc_test.rs", "// test\n"),
        ],
    );
    let site = Site::open(fixture.path()).expect("site");

    let call = McpToolCall::Excavate {
        site_path: site.root_path.to_string_lossy().to_string(),
        file_path: "src/calc.rs".to_string(),
        start_line: 1,
        end_line: 1,
    };

    let result = mcp.handle_tool_call(call).expect("mcp call");
    assert_eq!(result.finding.file_path, "src/calc.rs");
    assert!(!result.from_cached_field_note);

    // Verify dig was automatically recorded in Field Notes
    let notes = store.list_field_notes(&site.root_path).expect("list notes");
    assert_eq!(notes.len(), 1);
    assert!(notes[0].user_notes.contains("MCP"));
}

#[test]
fn test_mcp_returns_existing_field_note_if_saved() {
    let store = Arc::new(SqliteStore::in_memory().expect("store"));
    let provider = Arc::new(FakeProvider::new("Initial explanation"));
    let mcp = McpServer::new(store.clone(), provider.clone());

    let mut fixture = GitFixture::new();
    fixture.commit(
        "Initial commit",
        &[("src/main.rs", "fn main() { println!(\"hi\"); }\n")],
    );
    let site = Site::open(fixture.path()).expect("site");

    let range = LineRange { start: 1, end: 1 };
    let finding = excavate(&site, "src/main.rs", range, provider).expect("excavate");

    store
        .save_field_note(&site.root_path, &finding, "Developer custom note")
        .expect("save note");

    let call = McpToolCall::Excavate {
        site_path: site.root_path.to_string_lossy().to_string(),
        file_path: "src/main.rs".to_string(),
        start_line: 1,
        end_line: 1,
    };

    let result = mcp.handle_tool_call(call).expect("mcp call");
    assert!(result.from_cached_field_note);
    assert_eq!(result.finding.file_path, "src/main.rs");
}
