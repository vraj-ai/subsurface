use std::process::Command;
use std::sync::Arc;

use subsurface_engine::assessment::{
    ActivityKind, ActivityPreview, AssessmentPreview, AssessmentRun, AssessmentRunStatus,
    ProjectAssessment,
};
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::grade::{
    CorrectnessMetrics, MaintainabilityMetrics, Measurement, QualityDimension, QualityMeasurements,
    SecurityFindings,
};
use subsurface_engine::opportunity::{
    detect_dead_workaround, detect_missing_rationale, detect_test_gap, model_proposed_opportunity,
    opportunities_from_report, order_opportunities, Effort, Impact, ImprovementReceipt,
    Opportunity, OpportunityCategory, OpportunityRank, OpportunityState, ReceiptSource,
    Verification,
};
use subsurface_engine::project::Project;
use subsurface_engine::provider::FakeProvider;
use subsurface_engine::report::generate_site_report;
use subsurface_engine::store::SqliteStore;

#[test]
fn opportunity_lifecycle_transitions() {
    let mut lifecycle = opportunity("lifecycle", Impact::Critical);
    assert_eq!(lifecycle.state, OpportunityState::Detected);
    assert!(lifecycle
        .transition(
            OpportunityState::Published,
            "2026-08-23T00:01:00Z",
            receipt(ReceiptSource::Tooling, "invalid publish")
        )
        .is_err());
    assert_eq!(lifecycle.state, OpportunityState::Detected);
    assert_eq!(lifecycle.events.len(), 1);

    lifecycle
        .transition(
            OpportunityState::Prepared,
            "2026-08-23T00:02:00Z",
            receipt(ReceiptSource::Tooling, "candidate prepared"),
        )
        .unwrap();
    lifecycle
        .transition(
            OpportunityState::Verified,
            "2026-08-23T00:03:00Z",
            receipt(ReceiptSource::Tooling, "locked tests passed"),
        )
        .unwrap();
    assert_eq!(lifecycle.rank.verification, Verification::Verified);
    lifecycle
        .transition(
            OpportunityState::Published,
            "2026-08-23T00:04:00Z",
            receipt(ReceiptSource::Tooling, "issue 81"),
        )
        .unwrap();

    assert_eq!(lifecycle.events.len(), 4);
    assert_eq!(lifecycle.events[0].state, OpportunityState::Detected);
    assert_eq!(lifecycle.events[3].state, OpportunityState::Published);
    assert_eq!(lifecycle.events[2].at, "2026-08-23T00:03:00Z");
    assert_eq!(
        lifecycle.events[2].receipt.as_ref().unwrap().summary,
        "locked tests passed"
    );

    let mut failed = opportunity("failed", Impact::High);
    failed
        .transition(
            OpportunityState::Prepared,
            "2026-08-23T01:00:00Z",
            receipt(ReceiptSource::Tooling, "candidate prepared"),
        )
        .unwrap();
    failed
        .transition(
            OpportunityState::Failed,
            "2026-08-23T01:01:00Z",
            receipt(ReceiptSource::Tooling, "tests failed"),
        )
        .unwrap();
    assert_eq!(failed.rank.verification, Verification::Failed);
    failed
        .transition(
            OpportunityState::Dismissed,
            "2026-08-23T01:02:00Z",
            receipt(ReceiptSource::User, "not worth the risk"),
        )
        .unwrap();
    assert_eq!(failed.state, OpportunityState::Dismissed);

    let mut detected_dismissal = opportunity("detected-dismissal", Impact::Low);
    detected_dismissal
        .transition(
            OpportunityState::Dismissed,
            "2026-08-23T02:00:00Z",
            receipt(ReceiptSource::User, "dismissed before preparation"),
        )
        .unwrap();
    assert_eq!(detected_dismissal.state, OpportunityState::Dismissed);

    let mut prepared_dismissal = opportunity("prepared-dismissal", Impact::Low);
    prepared_dismissal
        .transition(
            OpportunityState::Prepared,
            "2026-08-23T03:00:00Z",
            receipt(ReceiptSource::Tooling, "candidate prepared"),
        )
        .unwrap();
    prepared_dismissal
        .transition(
            OpportunityState::Dismissed,
            "2026-08-23T03:01:00Z",
            receipt(ReceiptSource::User, "dismissed after preview"),
        )
        .unwrap();
    assert_eq!(prepared_dismissal.state, OpportunityState::Dismissed);
}

#[test]
fn detectors_emit_linked_opportunities() {
    let rank = rank(Impact::High);
    let dead = detect_dead_workaround(
        "finding-dead",
        "src/cache.rs",
        ImprovementReceipt::new(
            ReceiptSource::ModelProposed,
            "linked issue closed in commit abc123",
            "git:abc123",
        ),
        rank,
        "2026-08-23T00:00:00Z",
    );
    let rationale = detect_missing_rationale(
        "finding-why",
        "src/auth.rs",
        ImprovementReceipt::new(
            ReceiptSource::Tooling,
            "history and docs contain no rationale",
            "git-log:src/auth.rs",
        ),
        rank,
        "2026-08-23T00:00:00Z",
    );
    let gap = detect_test_gap(
        "finding-test",
        "src/parser.rs",
        "No co-committed test was found; this is a heuristic, not verified coverage.",
        rank,
        "2026-08-23T00:00:00Z",
    );
    let model = model_proposed_opportunity(
        "finding-model",
        "src/lib.rs",
        "Model suggests splitting this module",
        rank,
        "2026-08-23T00:00:00Z",
    );

    assert_eq!(dead.category, OpportunityCategory::DeadWorkaround);
    assert_eq!(dead.finding_ids, vec!["finding-dead"]);
    assert_eq!(dead.file_path, "src/cache.rs");
    assert_eq!(
        dead.events[0].receipt.as_ref().unwrap().source,
        ReceiptSource::Tooling
    );
    assert_eq!(rationale.category, OpportunityCategory::MissingRationale);
    assert_eq!(gap.category, OpportunityCategory::TestGap);
    assert_eq!(
        gap.events[0].receipt.as_ref().unwrap().source,
        ReceiptSource::Heuristic
    );
    assert!(gap.events[0]
        .receipt
        .as_ref()
        .unwrap()
        .summary
        .contains("not verified coverage"));
    assert_eq!(model.category, OpportunityCategory::ModelProposed);
    assert_eq!(
        model.events[0].receipt.as_ref().unwrap().source,
        ReceiptSource::ModelProposed
    );

    let mut fixture = GitFixture::new();
    fixture.commit(
        "Workaround for parser bug (Issue #99)",
        &[("src/workaround.rs", "fn workaround() {}\n")],
    );
    fixture.commit(
        "Closes #99 because upstream fixed the parser",
        &[("Cargo.toml", "# dependency updated\n")],
    );
    fixture.commit(
        "Add parser because token lookahead is required by the grammar",
        &[("src/parser.rs", "fn parse() {}\n")],
    );
    let project = Project::open(fixture.path()).unwrap();
    let report = generate_site_report(
        &project,
        None,
        Arc::new(FakeProvider::new("Evidence-backed rationale")),
    )
    .unwrap();
    let linked = opportunities_from_report(&report, rank);
    assert!(!linked.is_empty());
    assert!(linked.iter().all(|item| !item.finding_ids.is_empty()));
    assert!(linked.iter().any(|item| {
        item.category == OpportunityCategory::DeadWorkaround
            && item.events[0].receipt.as_ref().unwrap().source == ReceiptSource::Tooling
    }));
    assert!(linked.iter().any(|item| {
        item.category == OpportunityCategory::TestGap
            && item.events[0]
                .receipt
                .as_ref()
                .unwrap()
                .summary
                .contains("not verified coverage")
    }));
}

#[test]
fn ordering_uses_separate_fields_not_composite_score() {
    let mut low_impact = opportunity("low-impact", Impact::Low);
    low_impact.rank.verification = Verification::Verified;
    let mut high_effort = opportunity("high-effort", Impact::Critical);
    high_effort.rank.effort = Effort::Large;
    let mut low_effort = opportunity("low-effort", Impact::Critical);
    low_effort.rank.effort = Effort::Small;
    let mut high_verification = opportunity("high-verification", Impact::Critical);
    high_verification.rank.verification = Verification::Verified;
    high_verification.rank.effort = Effort::Large;
    let mut older = opportunity("older", Impact::Critical);
    older.rank.effort = Effort::Small;
    older.rank.age_days = 20;

    let ordered = order_opportunities(vec![
        low_impact,
        high_effort,
        low_effort,
        high_verification,
        older,
    ]);
    assert_eq!(
        ordered
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "high-verification",
            "older",
            "low-effort",
            "high-effort",
            "low-impact"
        ]
    );
    assert_eq!(ordered[0].rank.impact, Impact::Critical);
    assert_eq!(ordered[0].rank.verification, Verification::Verified);
    assert_eq!(ordered[0].rank.expected_grade_improvement, 3);
    assert_eq!(ordered[0].rank.effort, Effort::Large);
    assert_eq!(ordered[0].rank.age_days, 2);
    let serialized = serde_json::to_string(&ordered[0].rank).unwrap();
    assert!(!serialized.contains("composite"));
    assert!(!serialized.contains("score"));
}

#[test]
fn project_assessment_at_commit_with_history() {
    let mut fixture = GitFixture::new();
    let baseline_sha = fixture.commit("baseline", &[("src/lib.rs", "pub fn baseline() {}\n")]);
    let baseline_project = Project::open(fixture.path()).unwrap();
    let store = SqliteStore::in_memory().unwrap();
    let baseline = ProjectAssessment::at_project_head(
        &baseline_project,
        measurements(80).grade(None),
        vec![opportunity("baseline-opportunity", Impact::High)],
        "2026-08-22T00:00:00Z",
        None,
    )
    .unwrap();
    let current_sha = fixture.commit("improved", &[("src/lib.rs", "pub fn improved() {}\n")]);
    let current_project = Project::open(fixture.path()).unwrap();
    let status_before = git_status(fixture.path());
    let current = ProjectAssessment::at_project_head(
        &current_project,
        measurements(95).grade(None),
        vec![],
        "2026-08-23T00:00:00Z",
        Some(&baseline),
    )
    .unwrap();

    store.save_assessment(&baseline).unwrap();
    store.save_assessment(&current).unwrap();
    let history = store.list_assessments(&current.project_path).unwrap();

    assert_eq!(history.len(), 2);
    assert_eq!(baseline.commit_sha, baseline_sha);
    assert_eq!(current.commit_sha, current_sha);
    assert_eq!(history[0].commit_sha, current.commit_sha);
    assert_eq!(history[1].commit_sha, baseline.commit_sha);
    assert_eq!(history[0].grade, current.grade);
    assert_eq!(
        history[0].baseline_commit_sha.as_deref(),
        Some(baseline.commit_sha.as_str())
    );
    assert_eq!(history[0].overall_delta, Some(15));
    assert_eq!(git_status(fixture.path()), status_before);
    assert_eq!(
        history[0]
            .dimension_deltas
            .iter()
            .find(|delta| delta.dimension == QualityDimension::Simplicity)
            .unwrap()
            .score_delta,
        15
    );
}

#[test]
fn assessment_preview_and_cancellation() {
    let preview = AssessmentPreview::new(
        "3333333333333333333333333333333333333333",
        vec![
            ActivityPreview::provider("OpenCode Go", "Send selected Evidence for critique"),
            ActivityPreview::command("cargo test --workspace", "Run locked Project tests"),
        ],
    );
    assert_eq!(preview.activities[0].kind, ActivityKind::Provider);
    assert_eq!(preview.activities[1].kind, ActivityKind::Command);
    assert!(preview.activities[0].detail.contains("Evidence"));

    let mut run = AssessmentRun::new(preview);
    run.start().unwrap();
    run.complete_activity("Collected local git Evidence")
        .unwrap();
    run.cancel("Cancelled by user").unwrap();

    assert_eq!(run.status, AssessmentRunStatus::Cancelled);
    assert_eq!(run.progress.completed, 1);
    assert_eq!(run.progress.total, 2);
    assert!(run.partial_results);
    assert_eq!(run.results_label(), "Partial results");
    assert_eq!(run.status_detail.as_deref(), Some("Cancelled by user"));
}

fn opportunity(id: &str, impact: Impact) -> Opportunity {
    Opportunity::detected(
        id,
        "finding-1",
        "src/lib.rs",
        OpportunityCategory::MissingRationale,
        rank(impact),
        "2026-08-23T00:00:00Z",
        receipt(ReceiptSource::Tooling, "detected from git history"),
    )
}

fn rank(impact: Impact) -> OpportunityRank {
    OpportunityRank {
        impact,
        verification: Verification::NotRun,
        expected_grade_improvement: 3,
        effort: Effort::Medium,
        age_days: 2,
    }
}

fn receipt(source: ReceiptSource, summary: &str) -> ImprovementReceipt {
    ImprovementReceipt::new(source, summary, "test-receipt")
}

fn measurements(score: u8) -> QualityMeasurements {
    QualityMeasurements {
        correctness: Some(Measurement::new(
            CorrectnessMetrics {
                build_passed: true,
                locked_tests_passed: true,
            },
            "build and locked tests passed",
        )),
        test_protection: Some(Measurement::new(score as f64, "coverage")),
        security: Some(Measurement::new(
            SecurityFindings {
                blocker: 0,
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
            },
            "security scan",
        )),
        maintainability: Some(Measurement::new(
            MaintainabilityMetrics {
                maintainability_index: score as f64,
                max_changed_function_complexity: if score >= 95 { 5 } else { 15 },
            },
            "maintainability metrics",
        )),
        simplicity: Some(Measurement::new(score, "simplicity checks")),
        evidence_fit: Some(Measurement::new(score, "Evidence fit")),
    }
}

fn git_status(path: &std::path::Path) -> String {
    let output = Command::new("git")
        .current_dir(path)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
