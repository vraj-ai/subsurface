use std::sync::Arc;
use subsurface_engine::confidence::Confidence;
use subsurface_engine::evidence::LineRange;
use subsurface_engine::excavate::excavate;
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::provider::{FakeProvider, Provider, ProviderError};
use subsurface_engine::site::Site;
use subsurface_engine::store::SqliteStore;

struct ErroringProvider;
impl Provider for ErroringProvider {
    fn complete(&self, _prompt: &str) -> Result<String, ProviderError> {
        Err(ProviderError::Network("Connection timed out".into()))
    }
}

#[test]
fn test_excavate_no_rationale_fixture_asserts_nothing() {
    let mut fixture = GitFixture::new();
    fixture.commit(
        "fix",
        &[(
            "src/payment.rs",
            "pub fn process_payment() {\n    let retry_limit = 3;\n    println!(\"{}\", retry_limit);\n}\n",
        )],
    );
    fixture.commit(
        "wip",
        &[(
            "src/payment.rs",
            "pub fn process_payment() {\n    let retry_limit = 5;\n    println!(\"{}\", retry_limit);\n}\n",
        )],
    );

    let site = Site::open(fixture.path()).expect("open site");
    let provider = Arc::new(FakeProvider::new("Should not be used for no-rationale"));

    let range = LineRange { start: 2, end: 2 };
    let finding = excavate(&site, "src/payment.rs", range, provider).expect("excavate");

    // The most important test in v1:
    assert_eq!(finding.why.confidence, Confidence::None);
    assert!(
        finding.why.rationale.to_lowercase().contains("no recorded rationale")
            || finding.why.rationale.to_lowercase().contains("no rationale"),
        "Finding must explicitly state that no rationale was recorded, got: {}",
        finding.why.rationale
    );

    // What/When must still be present and accurate
    assert_eq!(finding.current_code.trim(), "let retry_limit = 5;");
    assert!(!finding.what_when.timeline.is_empty());
    assert!(finding.what_when.first_introduced_commit.is_some());
}

#[test]
fn test_excavate_stated_rationale_fixture() {
    let mut fixture = GitFixture::new();
    let c1 = fixture.commit(
        "Workaround for upstream issue #404: retry limit increased to 5 due to gateway latency",
        &[(
            "src/payment.rs",
            "pub fn process_payment() {\n    let retry_limit = 5;\n}\n",
        )],
    );

    let site = Site::open(fixture.path()).expect("open site");
    let provider = Arc::new(FakeProvider::new(
        "Rationale from commit: Gateway latency requires retry limit 5.",
    ));

    let range = LineRange { start: 2, end: 2 };
    let finding = excavate(&site, "src/payment.rs", range, provider).expect("excavate");

    assert_eq!(finding.why.confidence, Confidence::Stated);
    assert!(finding.why.rationale.contains("Gateway latency"));
    assert!(!finding.why.evidence_citations.is_empty());
    assert_eq!(finding.why.evidence_citations[0].commit_sha, c1);
}

#[test]
fn test_excavate_what_when_intact_on_provider_error() {
    let mut fixture = GitFixture::new();
    fixture.commit(
        "Add calculate function",
        &[("src/calc.rs", "fn calc() -> i32 {\n    42\n}\n")],
    );

    let site = Site::open(fixture.path()).expect("open site");
    let provider = Arc::new(ErroringProvider);

    let range = LineRange { start: 1, end: 3 };
    let finding = excavate(&site, "src/calc.rs", range, provider).expect("excavate");

    // What/When is derived locally from git with no provider involvement
    assert_eq!(finding.current_code, "fn calc() -> i32 {\n    42\n}");
    assert_eq!(finding.what_when.timeline.len(), 1);
    assert!(finding.what_when.first_introduced_commit.is_some());
}

#[test]
fn test_sqlite_store_field_notes_and_isolation() {
    let store = SqliteStore::in_memory().expect("open memory store");

    let mut fixture_a = GitFixture::new();
    fixture_a.commit("init", &[("src/lib.rs", "fn a() {}\n")]);
    let site_a = Site::open(fixture_a.path()).expect("site a");

    let mut fixture_b = GitFixture::new();
    fixture_b.commit("init", &[("src/lib.rs", "fn b() {}\n")]);
    let site_b = Site::open(fixture_b.path()).expect("site b");

    let provider = Arc::new(FakeProvider::new("Explanation"));
    let range = LineRange { start: 1, end: 1 };
    let finding = excavate(&site_a, "src/lib.rs", range, provider).expect("excavate");

    // Save field note for Site A
    let note_id = store
        .save_field_note(&site_a.root_path, &finding, "My personal observation")
        .expect("save field note");

    // Site A has 1 note
    let notes_a = store.list_field_notes(&site_a.root_path).expect("list site a");
    assert_eq!(notes_a.len(), 1);
    assert_eq!(notes_a[0].id, note_id);
    assert_eq!(notes_a[0].user_notes, "My personal observation");

    // Site B has 0 notes (strict isolation)
    let notes_b = store.list_field_notes(&site_b.root_path).expect("list site b");
    assert_eq!(notes_b.len(), 0);

    // Search
    let search_res = store
        .search_field_notes(&site_a.root_path, "observation")
        .expect("search");
    assert_eq!(search_res.len(), 1);

    // Delete
    let deleted = store.delete_field_note(&note_id).expect("delete");
    assert!(deleted);
    let empty_notes = store.list_field_notes(&site_a.root_path).expect("list");
    assert_eq!(empty_notes.len(), 0);
}

#[test]
fn test_store_does_not_modify_user_repo() {
    let store = SqliteStore::in_memory().expect("open store");
    let mut fixture = GitFixture::new();
    fixture.commit("init", &[("src/lib.rs", "fn run() {}\n")]);
    let site = Site::open(fixture.path()).expect("open site");

    let provider = Arc::new(FakeProvider::new("A rationale"));
    let range = LineRange { start: 1, end: 1 };
    let finding = excavate(&site, "src/lib.rs", range, provider).expect("excavate");

    store
        .save_field_note(&site.root_path, &finding, "Test note")
        .expect("save note");

    // Invariant: Subsurface NEVER writes into user repo
    let status_output = std::process::Command::new("git")
        .current_dir(fixture.path())
        .args(["status", "--porcelain"])
        .output()
        .expect("git status");
    let status_str = String::from_utf8_lossy(&status_output.stdout);
    assert!(
        status_str.trim().is_empty(),
        "User repository must remain completely untouched: {}",
        status_str
    );
}

#[test]
fn test_finding_cache_invalidation_on_touching_commit() {
    let store = SqliteStore::in_memory().expect("open store");
    let mut fixture = GitFixture::new();
    let c1 = fixture.commit("init", &[("src/a.rs", "fn a() {}\n"), ("src/b.rs", "fn b() {}\n")]);
    let site = Site::open(fixture.path()).expect("site");

    let provider = Arc::new(FakeProvider::new("A rationale"));
    let range = LineRange { start: 1, end: 1 };
    let finding = excavate(&site, "src/a.rs", range, provider).expect("excavate");

    store
        .cache_finding(&site.root_path, "src/a.rs", range, &c1, &finding)
        .expect("cache");

    // Cached finding retrieved when HEAD is c1
    let cached = store
        .get_cached_finding(&site.root_path, "src/a.rs", range, &c1)
        .expect("get cache");
    assert!(cached.is_some());

    // Commit 2 modifies unrelated file src/b.rs
    let c2 = fixture.commit("modify b", &[("src/b.rs", "fn b() { /* edited */ }\n")]);
    // Cache is still valid because src/a.rs was not touched between c1 and c2
    let site2 = Site::open(fixture.path()).expect("site2");
    assert!(store
        .is_cache_valid_for_head(&site2.root_path, "src/a.rs", &c1, &c2)
        .unwrap());

    // Commit 3 modifies src/a.rs
    let c3 = fixture.commit("modify a", &[("src/a.rs", "fn a() { /* changed */ }\n")]);
    let site3 = Site::open(fixture.path()).expect("site3");
    assert!(!store
        .is_cache_valid_for_head(&site3.root_path, "src/a.rs", &c1, &c3)
        .unwrap());
}
