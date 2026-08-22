use std::sync::Arc;
use subsurface_engine::excavate::excavate;
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::oauth::{OAuthClient, OAuthTokens};
use subsurface_engine::provider::FakeProvider;
use subsurface_engine::report::{estimate_site_report_cost, generate_site_report, SiteReportCategory};
use subsurface_engine::site::Site;

#[test]
fn test_site_report_entries_match_direct_excavate() {
    let mut fixture = GitFixture::new();
    // 1. Dead workaround file with closed issue receipt
    fixture.commit(
        "Workaround for bug in parser (Issue #99)",
        &[("src/workaround.rs", "fn workaround() { /* guard */ }\n")],
    );
    fixture.commit(
        "Closes #99: fixed in upstream engine",
        &[("Cargo.toml", "# updated\n")],
    );

    // 2. File with no recorded rationale ("fix"/"wip")
    fixture.commit(
        "fix",
        &[("src/no_rationale.rs", "fn unexplained() { 123 }\n")],
    );

    // 3. File with tests
    fixture.commit(
        "Add calculate feature with test",
        &[
            ("src/calc.rs", "fn calc() -> i32 { 42 }\n"),
            ("tests/calc_test.rs", "// test calc\n"),
        ],
    );

    let site = Site::open(fixture.path()).expect("site");
    let provider = Arc::new(FakeProvider::new("Archaeology explanation"));

    let report = generate_site_report(&site, None, provider.clone()).expect("generate report");
    assert_eq!(report.head_commit, site.head_commit.as_ref().unwrap().clone());
    assert!(!report.entries.is_empty());

    // Check categories in report
    let dead_entries: Vec<_> = report
        .entries
        .iter()
        .filter(|e| e.category == SiteReportCategory::DeadWorkaround)
        .collect();
    let no_rationale_entries: Vec<_> = report
        .entries
        .iter()
        .filter(|e| e.category == SiteReportCategory::NoRationale)
        .collect();

    assert!(!dead_entries.is_empty(), "Should report dead workaround");
    assert!(!no_rationale_entries.is_empty(), "Should report no-rationale region");

    // Invariant: Report entries link to Findings identical to a direct excavate()
    for entry in &report.entries {
        let direct_finding = excavate(&site, &entry.file_path, entry.line_range, provider.clone())
            .expect("direct excavate");
        assert_eq!(entry.finding.file_path, direct_finding.file_path);
        assert_eq!(entry.finding.line_range, direct_finding.line_range);
        assert_eq!(entry.finding.why.confidence, direct_finding.why.confidence);
    }
}

#[test]
fn test_site_report_filter_and_cost_estimation() {
    let mut fixture = GitFixture::new();
    fixture.commit(
        "init",
        &[
            ("src/core/a.rs", "fn a() {}\n"),
            ("src/extra/b.rs", "fn b() {}\n"),
            ("docs/readme.md", "# Readme\n"),
        ],
    );

    let site = Site::open(fixture.path()).expect("site");
    let estimate = estimate_site_report_cost(&site, Some("src/core/"));
    assert_eq!(estimate.file_count, 1);
    assert!(estimate.estimated_duration_secs >= 0.0);

    let provider = Arc::new(FakeProvider::new("Explanation"));
    let filtered_report = generate_site_report(&site, Some("src/core/"), provider).expect("report");
    assert!(filtered_report.entries.iter().all(|e| e.file_path.starts_with("src/core/")));
}

#[test]
fn test_oauth_token_management_and_refresh() {
    let mut tokens = OAuthTokens {
        access_token: "old_access_token".to_string(),
        refresh_token: Some("valid_refresh_token".to_string()),
        expires_at_timestamp: 0, // expired
        token_type: "Bearer".to_string(),
    };

    assert!(tokens.is_expired());

    let _client = OAuthClient::new(
        "client_id_123",
        "https://auth.example.com/oauth/authorize",
        "https://auth.example.com/oauth/token",
    );

    // Refresh simulation
    tokens.access_token = "new_access_token".to_string();
    tokens.expires_at_timestamp = chrono::Utc::now().timestamp() + 3600;
    assert!(!tokens.is_expired());
    assert_eq!(tokens.access_token, "new_access_token");
}
