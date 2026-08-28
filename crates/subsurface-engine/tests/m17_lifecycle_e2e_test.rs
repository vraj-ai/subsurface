mod support;

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use subsurface_engine::assessment::{
    ActivityPreview, AssessmentPreview, AssessmentRun, AssessmentRunStatus, ProjectAssessment,
};
use subsurface_engine::candidate::{fingerprint_project, prepare_candidate, CandidateEdit};
use subsurface_engine::command::{
    AllowedCommand, CommandAllowlist, CommandOutcome, IsolatedCommand, IsolatedRunner,
    ResourceBounds,
};
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::github::{
    fingerprint_marker, infer_github_destination, publish_work_item, render_work_item, GitHubAuth,
    GitHubAuthMethod, GitHubClient, PublishOutcome, WorkItemDestination, WorkItemDraft,
};
use subsurface_engine::grade::{
    CorrectnessMetrics, Grade, LetterGrade, MaintainabilityMetrics, Measurement,
    QualityMeasurements, SecurityFindings,
};
use subsurface_engine::opportunity::{
    opportunities_from_report, Effort, Impact, OpportunityCategory, OpportunityRank,
    OpportunityState, OpportunityStatus, ReceiptSource, Verification,
};
use subsurface_engine::project::Project;
use subsurface_engine::provider::FakeProvider;
use subsurface_engine::receipt::{
    compare_baseline_and_candidate, Check, ImprovementReceipt, ReceiptVerdict,
};
use subsurface_engine::report::generate_site_report;
use support::{LocalHttpFake, StubResponse};

fn check_script() -> &'static str {
    concat!(
        "import pathlib, sys\n",
        "text = pathlib.Path('src/lib.rs').read_text()\n",
        "sys.exit(0 if 'fn improved' in text else 1)\n",
    )
}

fn bounds() -> ResourceBounds {
    ResourceBounds {
        timeout: Duration::from_secs(2),
        max_output_bytes: 4_096,
        max_cpu: Duration::from_secs(2),
        max_concurrency: 1,
    }
}

fn allowlist() -> CommandAllowlist {
    CommandAllowlist::new(vec![
        AllowedCommand::new("python3").with_arg_prefix(["check.py"])
    ])
}

fn locked_check() -> Check {
    Check::new(
        "locked-checks",
        IsolatedCommand::new("python3", ["check.py"]),
    )
}

fn rank() -> OpportunityRank {
    OpportunityRank {
        impact: Impact::High,
        verification: Verification::NotRun,
        expected_grade_improvement: 12,
        effort: Effort::Small,
        age_days: 1,
    }
}

fn measurements(locked_tests_passed: bool, score: u8) -> QualityMeasurements {
    QualityMeasurements {
        correctness: Some(Measurement::new(
            CorrectnessMetrics {
                build_passed: true,
                locked_tests_passed,
            },
            "build and locked tests",
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

fn publish_auth() -> GitHubAuth {
    GitHubAuth {
        method: GitHubAuthMethod::Token,
        token: "gho_test_token".into(),
        attempted: vec![GitHubAuthMethod::Token],
    }
}

fn empty_search() -> StubResponse {
    StubResponse::json(200, r#"{"total_count":0,"items":[]}"#)
}

fn created_issue(number: u64, title: &str, body: &str) -> StubResponse {
    let escaped_title = escape_json(title);
    let escaped_body = escape_json(body);
    StubResponse::json(
        201,
        format!(
            r#"{{"number":{number},"html_url":"http://127.0.0.1/issues/{number}","title":"{escaped_title}","body":"{escaped_body}","state":"open"}}"#
        ),
    )
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn opportunity_receipt(
    source: ReceiptSource,
    summary: &str,
    reference: &str,
) -> subsurface_engine::opportunity::ImprovementReceipt {
    subsurface_engine::opportunity::ImprovementReceipt::new(source, summary, reference)
}

/// One GitFixture through Assess → prepare → verify → grade → receipt → publish.
#[test]
fn git_fixture_lifecycle_assess_prepare_verify_grade_receipt_publish() {
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
        "initial lib",
        &[
            ("src/lib.rs", "fn broken() {}\n"),
            ("check.py", check_script()),
        ],
    );
    fixture.add_remote("origin", "https://github.com/acme/widgets.git");

    let scratch = fixture.path().join("scratch.txt");
    fs::write(&scratch, "local only\n").expect("write uncommitted file");

    let project = Project::open(fixture.path()).expect("open project");
    let before = fingerprint_project(&project).expect("fingerprint before");
    assert_eq!(before.head.len(), 40);
    assert!(
        before.porcelain.contains("scratch.txt"),
        "expected dirty porcelain before lifecycle, got {:?}",
        before.porcelain
    );

    // Assess
    let mut run = AssessmentRun::new(AssessmentPreview::new(
        before.head.clone(),
        vec![
            ActivityPreview::command("git log", "Collect recorded Evidence"),
            ActivityPreview::command("python3 check.py", "Run locked Project checks"),
        ],
    ));
    run.start().expect("start assessment");
    run.complete_activity("Collected local git Evidence")
        .expect("record evidence");
    run.complete_activity("Baseline locked checks did not prove the Project")
        .expect("record baseline checks");
    assert_eq!(run.status, AssessmentRunStatus::Completed);

    let report = generate_site_report(
        &project,
        Some("src/lib.rs"),
        Arc::new(FakeProvider::new("Evidence-backed rationale")),
    )
    .expect("assess project");
    assert_eq!(report.head_commit, before.head);
    let mut opportunities = opportunities_from_report(&report, rank());
    assert!(
        !opportunities.is_empty(),
        "assessment must surface at least one Opportunity"
    );
    let baseline_grade = measurements(false, 70).grade(None);
    assert_eq!(baseline_grade.overall, Grade::Letter(LetterGrade::F));
    let assessment = ProjectAssessment::at_project_head(
        &project,
        baseline_grade,
        opportunities.clone(),
        "2026-08-28T00:00:00Z",
        None,
    )
    .expect("project assessment at HEAD");
    assert_eq!(assessment.commit_sha, before.head);
    assert_eq!(assessment.opportunities.len(), opportunities.len());
    let after_assess = fingerprint_project(&project).expect("fingerprint after assess");
    assert_eq!(after_assess, before);

    let mut opportunity = opportunities
        .drain(..)
        .find(|item| item.file_path == "src/lib.rs")
        .expect("lifecycle Opportunity for src/lib.rs");
    assert_eq!(opportunity.state, OpportunityState::Detected);
    assert_eq!(opportunity.file_path, "src/lib.rs");

    let edits = [CandidateEdit {
        path: "src/lib.rs".into(),
        contents: "fn improved() {}\n".into(),
    }];

    // Prepare
    let prepared = prepare_candidate(&project, &edits).expect("prepare candidate");
    assert_ne!(prepared.clone_path, project.root_path);
    assert_eq!(prepared.base_commit, before.head);
    assert_eq!(
        fs::read_to_string(prepared.clone_path.join("src/lib.rs")).expect("read clone file"),
        "fn improved() {}\n"
    );
    assert_eq!(
        fs::read_to_string(project.root_path.join("src/lib.rs")).expect("read project file"),
        "fn broken() {}\n"
    );
    opportunity
        .transition(
            OpportunityState::Prepared,
            "2026-08-28T00:01:00Z",
            opportunity_receipt(
                ReceiptSource::Tooling,
                "candidate prepared in a disposable clone",
                "prepare:src/lib.rs",
            ),
        )
        .expect("Detected → Prepared");
    let after_prepare = fingerprint_project(&project).expect("fingerprint after prepare");
    assert_eq!(after_prepare, before);

    // Verify
    let runner = IsolatedRunner::new(
        project.clone(),
        prepared.clone_path.clone(),
        allowlist(),
        bounds(),
    );
    let verification = runner
        .run(&locked_check().command)
        .expect("verify candidate checks");
    assert_eq!(verification.outcome, CommandOutcome::Succeeded);
    assert_eq!(verification.fingerprint_before, before);
    assert_eq!(verification.fingerprint_after, before);
    opportunity
        .transition(
            OpportunityState::Verified,
            "2026-08-28T00:02:00Z",
            opportunity_receipt(
                ReceiptSource::Tooling,
                "locked checks proved the prepared change",
                "verify:locked-checks",
            ),
        )
        .expect("Prepared → Verified");
    assert_eq!(opportunity.rank.verification, Verification::Verified);
    drop(prepared);
    let after_verify = fingerprint_project(&project).expect("fingerprint after verify");
    assert_eq!(after_verify, before);

    // Grade
    let candidate_grade = measurements(true, 95).grade(None);
    assert_eq!(
        candidate_grade.overall,
        Grade::Letter(LetterGrade::APlus)
    );
    assert!(candidate_grade.missing_dimensions.is_empty());
    assert!(candidate_grade.hard_failures.is_empty());
    assert!(candidate_grade.automation_eligible);
    let after_grade = fingerprint_project(&project).expect("fingerprint after grade");
    assert_eq!(after_grade, before);

    // Receipt
    let receipt: ImprovementReceipt =
        compare_baseline_and_candidate(&project, &edits, &[locked_check()], allowlist(), bounds())
            .expect("improvement receipt");
    assert_eq!(receipt.verdict, ReceiptVerdict::Improved);
    assert_eq!(receipt.base_commit, before.head);
    assert_eq!(receipt.project_fingerprint, before);
    assert_eq!(receipt.proving_checks, vec!["locked-checks".to_string()]);
    assert!(
        receipt
            .improved
            .iter()
            .any(|item| item.contains("locked-checks")),
        "receipt should name what improved, got {:?}",
        receipt.improved
    );
    assert!(receipt.remaining.is_empty());
    assert!(!receipt.comparisons[0].baseline.proved);
    assert!(receipt.comparisons[0].candidate.proved);
    let after_receipt = fingerprint_project(&project).expect("fingerprint after receipt");
    assert_eq!(after_receipt, before);

    // Publish
    let destination = infer_github_destination(&project).expect("infer GitHub destination");
    assert_eq!(destination.kind(), WorkItemDestination::GitHub);
    opportunity.draft = WorkItemDraft {
        id: opportunity.id.clone(),
        title: format!("Improve {}", opportunity.file_path),
        category: match opportunity.category {
            OpportunityCategory::DeadWorkaround => "dead-workaround",
            OpportunityCategory::MissingRationale => "missing-rationale",
            OpportunityCategory::TestGap => "test-gap",
            OpportunityCategory::ModelProposed => "model-proposed",
        }
        .into(),
        file_path: opportunity.file_path.clone(),
        summary: opportunity
            .events
            .first()
            .and_then(|event| event.receipt.as_ref())
            .map(|item| item.summary.clone())
            .unwrap_or_else(|| "assessed Opportunity".into()),
        evidence_ids: opportunity.finding_ids.clone(),
        base_commit: None,
        receipt: None,
    }
    .with_improvement_receipt(&receipt);
    let rendered = render_work_item(&destination, &opportunity.draft);
    let server = LocalHttpFake::start_with(vec![
        empty_search(),
        created_issue(81, &rendered.title, &rendered.body),
    ]);
    let client = GitHubClient::new()
        .with_base_url(format!("http://{}", server.address()))
        .with_timeout(Duration::from_secs(2));
    let published = publish_work_item(&client, &destination, &publish_auth(), &opportunity.draft)
        .expect("publish Work Item");
    assert_eq!(published.number, 81);
    assert_eq!(published.outcome, PublishOutcome::Created);
    assert_eq!(published.fingerprint, rendered.fingerprint);
    assert!(rendered
        .body
        .contains(&fingerprint_marker(&rendered.fingerprint)));
    assert!(
        rendered.body.contains("Work Item"),
        "published body must name a Work Item"
    );
    assert!(
        !rendered.body.contains("Opportunity"),
        "published body must not call the Work Item an Opportunity"
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("GET /search/issues?"));
    assert!(requests[1].starts_with("POST /repos/acme/widgets/issues "));
    assert!(
        !requests
            .iter()
            .any(|request| request.contains("api.github.com")),
        "tests must not write to live GitHub"
    );
    assert!(server.address().ip().is_loopback());

    opportunity
        .transition(
            OpportunityState::Published,
            "2026-08-28T00:03:00Z",
            opportunity_receipt(ReceiptSource::Tooling, "issue 81", "github:acme/widgets#81"),
        )
        .expect("Verified → Published");
    opportunity.record_publication(published);
    assert_eq!(opportunity.state, OpportunityState::Published);
    assert_eq!(
        opportunity.status,
        OpportunityStatus::Published
    );

    let after = fingerprint_project(&project).expect("fingerprint after publish");
    assert_eq!(after, before);
    assert_eq!(
        fs::read_to_string(project.root_path.join("src/lib.rs")).expect("read project file"),
        "fn broken() {}\n"
    );
    assert_eq!(
        fs::read_to_string(&scratch).expect("read uncommitted file"),
        "local only\n"
    );
}
