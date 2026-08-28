mod support;

use std::time::Duration;

use subsurface_engine::automation::{
    AutoPublishBlock, AutoPublishCandidate, AutoPublishDecision, AutoPublishError,
    AutoPublishEvent, AutoPublisher, QualityGrade,
};
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::github::{
    fetch_work_item_state, fingerprint_marker, infer_github_destination, preview_work_item,
    publish_work_item_preview, GitHubAuth, GitHubAuthMethod, GitHubClient, GitHubDestination,
    PublishOutcome, WorkItemDraft, WorkItemTrackerState,
};
use subsurface_engine::opportunity::{Opportunity, OpportunityStatus, Reassessment};
use subsurface_engine::project::Project;
use subsurface_engine::receipt::{ImprovementReceipt, ReceiptVerdict};
use support::{LocalHttpFake, StubResponse};

fn github_project() -> (GitFixture, Project) {
    let mut fixture = GitFixture::new();
    fixture.commit("initial", &[("src/auth.rs", "fn auth() {}\n")]);
    fixture.add_remote("origin", "https://github.com/acme/widgets.git");
    let project = Project::open(fixture.path()).expect("open github project");
    (fixture, project)
}

fn publish_auth() -> GitHubAuth {
    GitHubAuth {
        method: GitHubAuthMethod::Token,
        token: "gho_test_token".into(),
        attempted: vec![GitHubAuthMethod::Token],
    }
}

fn approved_draft(id: &str) -> WorkItemDraft {
    WorkItemDraft {
        id: id.into(),
        title: "Restore rationale for auth".into(),
        category: "missing-rationale".into(),
        file_path: "src/auth.rs".into(),
        summary: "history and docs contain no rationale".into(),
        evidence_ids: vec!["src/auth.rs:1-20@abc123".into()],
        base_commit: None,
        receipt: None,
    }
    .with_improvement_receipt(&ImprovementReceipt {
        base_commit: "abc123".into(),
        improved: vec!["locked tests now prove the prepared change".into()],
        proving_checks: vec!["locked-tests".into()],
        remaining: vec![],
        comparisons: vec![],
        verdict: ReceiptVerdict::Improved,
        project_fingerprint: subsurface_engine::candidate::GitFingerprint {
            head: "abc123".into(),
            porcelain: String::new(),
        },
    })
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

fn closed_issue(number: u64, title: &str, body: &str) -> StubResponse {
    let escaped_title = escape_json(title);
    let escaped_body = escape_json(body);
    StubResponse::json(
        200,
        format!(
            r#"{{"number":{number},"html_url":"http://127.0.0.1/issues/{number}","title":"{escaped_title}","body":"{escaped_body}","state":"closed"}}"#
        ),
    )
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[test]
fn closed_work_item_does_not_resolve_without_reassessment() {
    let (_fixture, project) = github_project();
    let destination = infer_github_destination(&project).expect("infer destination");
    let draft = approved_draft("missing-rationale:src/auth.rs:1-20@abc123");
    let mut opportunity = Opportunity::verified(draft);
    assert_eq!(opportunity.status, OpportunityStatus::Verified);
    assert!(!opportunity.is_resolved());

    let mut preview = opportunity.preview(&destination);
    preview.edit_title("Restore documented rationale for auth");
    let original_body = preview.body.clone();
    preview.edit_body(format!(
        "Edited Work Item body for `src/auth.rs`.\n\n{}",
        original_body
    ));
    assert!(
        preview
            .body
            .contains(&fingerprint_marker(&preview.fingerprint)),
        "edited preview must keep the fingerprint marker"
    );
    assert_eq!(preview.title, "Restore documented rationale for auth");

    let server = LocalHttpFake::start_with(vec![
        empty_search(),
        created_issue(81, &preview.title, &preview.body),
        closed_issue(81, &preview.title, &preview.body),
    ]);
    let client = GitHubClient::new()
        .with_base_url(format!("http://{}", server.address()))
        .with_timeout(Duration::from_secs(2));

    let published = publish_work_item_preview(&client, &publish_auth(), &preview)
        .expect("publish edited preview");
    assert_eq!(published.number, 81);
    assert_eq!(published.outcome, PublishOutcome::Created);
    opportunity.record_publication(published.clone());
    assert_eq!(opportunity.status, OpportunityStatus::Published);
    assert_eq!(
        opportunity.tracker_state(),
        Some(WorkItemTrackerState::Open)
    );
    assert!(
        !opportunity.is_resolved(),
        "publication is not Opportunity resolution"
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("GET /search/issues?"));
    assert!(requests[1].starts_with("POST /repos/acme/widgets/issues "));
    assert!(
        requests[1].contains("Restore documented rationale for auth"),
        "manual preview edits must be what GitHub receives"
    );
    assert!(
        !requests
            .iter()
            .any(|request| request.contains("api.github.com")),
        "tests must not write to live GitHub"
    );
    assert!(server.address().ip().is_loopback());

    let tracker_state =
        fetch_work_item_state(&client, &destination, &publish_auth(), published.number)
            .expect("fetch closed Work Item");
    assert_eq!(tracker_state, WorkItemTrackerState::Closed);
    opportunity.observe_tracker_state(tracker_state);
    assert_eq!(
        opportunity.tracker_state(),
        Some(WorkItemTrackerState::Closed)
    );
    assert_eq!(opportunity.status, OpportunityStatus::Published);
    assert!(
        !opportunity.is_resolved(),
        "closing a Work Item changes tracker state only"
    );

    opportunity.apply_reassessment(Reassessment {
        proves_improvement_receipt: false,
    });
    assert!(
        !opportunity.is_resolved(),
        "a reassessment that does not prove the receipt must not resolve"
    );

    opportunity.apply_reassessment(Reassessment {
        proves_improvement_receipt: true,
    });
    assert!(
        opportunity.is_resolved(),
        "Opportunity resolves only after a fresh Assessment proves its Improvement Receipt"
    );
    assert_eq!(opportunity.status, OpportunityStatus::Published);

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[2].starts_with("GET /repos/acme/widgets/issues/81 "));
}

fn eligible_publisher() -> AutoPublisher {
    let mut publisher = AutoPublisher::default();
    publisher.enable();
    publisher
        .settings_mut()
        .enable_category("missing-rationale");
    publisher
        .settings_mut()
        .record_proven_example("missing-rationale");
    publisher
}

fn candidate<'a>(
    opportunity: &'a Opportunity,
    destination: &'a GitHubDestination,
    grade: QualityGrade,
    model_only: bool,
) -> AutoPublishCandidate<'a> {
    AutoPublishCandidate {
        opportunity,
        destination,
        grade,
        model_only,
    }
}

fn assert_no_live_github(server: &LocalHttpFake) {
    let requests = server.requests();
    assert!(
        !requests
            .iter()
            .any(|request| request.contains("api.github.com")),
        "tests must not write to live GitHub"
    );
    assert!(server.address().ip().is_loopback());
}

fn create_server_for(
    destination: &GitHubDestination,
    draft: &WorkItemDraft,
    number: u64,
) -> LocalHttpFake {
    let preview = preview_work_item(destination, draft);
    LocalHttpFake::start_with(vec![
        empty_search(),
        created_issue(number, &preview.title, &preview.body),
    ])
}

#[test]
fn auto_publish_is_off_by_default() {
    let (_fixture, project) = github_project();
    let destination = infer_github_destination(&project).expect("infer destination");
    let mut opportunity =
        Opportunity::verified(approved_draft("missing-rationale:src/auth.rs:1-20@abc123"));
    let publisher = AutoPublisher::default();

    assert!(!publisher.settings().is_enabled());
    assert_eq!(publisher.settings().min_grade(), QualityGrade::APlus);

    let mut publisher = publisher;
    let decision = publisher.consider(&candidate(
        &opportunity,
        &destination,
        QualityGrade::APlus,
        false,
    ));
    assert_eq!(
        decision,
        AutoPublishDecision::Blocked(AutoPublishBlock::Disabled)
    );
    assert!(publisher.pending().is_none());

    let server = LocalHttpFake::start_with(vec![StubResponse::json(
        500,
        r#"{"error":"auto-publish off must not write"}"#,
    )]);
    let client = GitHubClient::new()
        .with_base_url(format!("http://{}", server.address()))
        .with_timeout(Duration::from_secs(2));
    let error = publisher
        .publish_due(&client, &publish_auth(), &mut opportunity)
        .expect_err("off-by-default must not publish");
    assert!(matches!(error, AutoPublishError::NoPendingCountdown));
    assert!(server.requests().is_empty());
    assert_no_live_github(&server);
    assert_eq!(opportunity.status, OpportunityStatus::Verified);
}

#[test]
fn auto_publish_requires_category_enable_and_proven_example() {
    let (_fixture, project) = github_project();
    let destination = infer_github_destination(&project).expect("infer destination");
    let opportunity =
        Opportunity::verified(approved_draft("missing-rationale:src/auth.rs:1-20@abc123"));
    let mut publisher = AutoPublisher::default();
    publisher.enable();

    let decision = publisher.consider(&candidate(
        &opportunity,
        &destination,
        QualityGrade::APlus,
        false,
    ));
    assert_eq!(
        decision,
        AutoPublishDecision::Blocked(AutoPublishBlock::CategoryNotEnabled {
            category: "missing-rationale".into(),
        })
    );

    publisher
        .settings_mut()
        .enable_category("missing-rationale");
    let decision = publisher.consider(&candidate(
        &opportunity,
        &destination,
        QualityGrade::APlus,
        false,
    ));
    assert_eq!(
        decision,
        AutoPublishDecision::Blocked(AutoPublishBlock::NoProvenExample {
            category: "missing-rationale".into(),
        })
    );
    assert!(publisher.pending().is_none());
}

#[test]
fn auto_publish_excludes_incomplete_and_model_only() {
    let (_fixture, project) = github_project();
    let destination = infer_github_destination(&project).expect("infer destination");
    let opportunity =
        Opportunity::verified(approved_draft("missing-rationale:src/auth.rs:1-20@abc123"));
    let mut publisher = eligible_publisher();
    publisher.settings_mut().set_min_grade(QualityGrade::F);

    let incomplete = publisher.consider(&candidate(
        &opportunity,
        &destination,
        QualityGrade::Incomplete,
        false,
    ));
    assert_eq!(
        incomplete,
        AutoPublishDecision::Blocked(AutoPublishBlock::Incomplete)
    );

    let model_only = publisher.consider(&candidate(
        &opportunity,
        &destination,
        QualityGrade::APlus,
        true,
    ));
    assert_eq!(
        model_only,
        AutoPublishDecision::Blocked(AutoPublishBlock::ModelOnly)
    );
    assert!(publisher.pending().is_none());
    assert!(
        !QualityGrade::Incomplete.meets_minimum(QualityGrade::F),
        "Incomplete must never meet a letter-grade floor"
    );
}

#[test]
fn auto_publish_minimum_grade_includes_equal_and_stronger() {
    assert!(QualityGrade::C.meets_minimum(QualityGrade::C));
    assert!(QualityGrade::B.meets_minimum(QualityGrade::C));
    assert!(QualityGrade::A.meets_minimum(QualityGrade::C));
    assert!(QualityGrade::APlus.meets_minimum(QualityGrade::C));
    assert!(!QualityGrade::D.meets_minimum(QualityGrade::C));
    assert!(!QualityGrade::A.meets_minimum(QualityGrade::APlus));
    assert!(QualityGrade::APlus.meets_minimum(QualityGrade::APlus));

    let (_fixture, project) = github_project();
    let destination = infer_github_destination(&project).expect("infer destination");
    let opportunity =
        Opportunity::verified(approved_draft("missing-rationale:src/auth.rs:1-20@abc123"));
    let mut publisher = eligible_publisher();
    publisher.settings_mut().set_min_grade(QualityGrade::C);

    match publisher.consider(&candidate(
        &opportunity,
        &destination,
        QualityGrade::C,
        false,
    )) {
        AutoPublishDecision::Countdown(countdown) => {
            assert_eq!(countdown.grade, QualityGrade::C);
            assert_eq!(countdown.destination.owner, "acme");
            assert_eq!(countdown.destination.repo, "widgets");
        }
        other => panic!("C should meet minimum C, got {other:?}"),
    }
    publisher.cancel();

    publisher.settings_mut().set_min_grade(QualityGrade::APlus);
    let blocked = publisher.consider(&candidate(
        &opportunity,
        &destination,
        QualityGrade::A,
        false,
    ));
    assert_eq!(
        blocked,
        AutoPublishDecision::Blocked(AutoPublishBlock::GradeBelowMinimum {
            grade: QualityGrade::A,
            minimum: QualityGrade::APlus,
        })
    );
}

#[test]
fn auto_publish_countdown_can_be_cancelled() {
    let (_fixture, project) = github_project();
    let destination = infer_github_destination(&project).expect("infer destination");
    let mut opportunity =
        Opportunity::verified(approved_draft("missing-rationale:src/auth.rs:1-20@abc123"));
    let mut publisher = eligible_publisher();
    let decision = publisher.consider(&candidate(
        &opportunity,
        &destination,
        QualityGrade::APlus,
        false,
    ));
    match decision {
        AutoPublishDecision::Countdown(countdown) => {
            assert_eq!(countdown.grade, QualityGrade::APlus);
            assert_eq!(countdown.destination.owner, "acme");
            assert_eq!(countdown.opportunity_id, opportunity.id);
        }
        other => panic!("expected countdown, got {other:?}"),
    }

    let cancelled = publisher.cancel().expect("countdown to cancel");
    assert_eq!(cancelled.opportunity_id, opportunity.id);
    assert!(publisher.pending().is_none());
    assert!(publisher
        .log()
        .iter()
        .any(|event| matches!(event, AutoPublishEvent::Cancelled { .. })));

    let server = LocalHttpFake::start_with(vec![StubResponse::json(
        500,
        r#"{"error":"cancelled countdown must not write"}"#,
    )]);
    let client = GitHubClient::new()
        .with_base_url(format!("http://{}", server.address()))
        .with_timeout(Duration::from_secs(2));
    let error = publisher
        .publish_due(&client, &publish_auth(), &mut opportunity)
        .expect_err("cancelled countdown must not publish");
    assert!(matches!(error, AutoPublishError::NoPendingCountdown));
    assert!(server.requests().is_empty());
    assert_no_live_github(&server);
    assert_eq!(opportunity.status, OpportunityStatus::Verified);
}

#[test]
fn auto_publish_happy_path_respects_daily_limit_and_log() {
    let (_fixture, project) = github_project();
    let destination = infer_github_destination(&project).expect("infer destination");
    let mut first =
        Opportunity::verified(approved_draft("missing-rationale:src/auth.rs:1-20@abc123"));
    let mut publisher = eligible_publisher();
    publisher.settings_mut().set_daily_limit(1);

    match publisher.consider(&candidate(&first, &destination, QualityGrade::APlus, false)) {
        AutoPublishDecision::Countdown(countdown) => {
            assert_eq!(countdown.destination.owner, "acme");
            assert_eq!(countdown.grade, QualityGrade::APlus);
        }
        other => panic!("expected countdown, got {other:?}"),
    }

    let server = create_server_for(&destination, &first.draft, 91);
    let client = GitHubClient::new()
        .with_base_url(format!("http://{}", server.address()))
        .with_timeout(Duration::from_secs(2));
    let published = publisher
        .publish_due(&client, &publish_auth(), &mut first)
        .expect("eligible auto-publish");
    assert_eq!(published.number, 91);
    assert_eq!(published.outcome, PublishOutcome::Created);
    assert_eq!(first.status, OpportunityStatus::Published);
    assert_eq!(publisher.published_today(), 1);
    assert!(publisher
        .log()
        .iter()
        .any(|event| matches!(event, AutoPublishEvent::Published { number: 91, .. })));

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("GET /search/issues?"));
    assert!(requests[1].starts_with("POST /repos/acme/widgets/issues "));
    assert_no_live_github(&server);

    let second =
        Opportunity::verified(approved_draft("missing-rationale:src/auth.rs:21-40@def456"));
    let blocked = publisher.consider(&candidate(
        &second,
        &destination,
        QualityGrade::APlus,
        false,
    ));
    assert_eq!(
        blocked,
        AutoPublishDecision::Blocked(AutoPublishBlock::DailyLimitReached { limit: 1 })
    );

    publisher.start_new_day();
    match publisher.consider(&candidate(
        &second,
        &destination,
        QualityGrade::APlus,
        false,
    )) {
        AutoPublishDecision::Countdown(_) => {}
        other => panic!("new day should reset the daily limit, got {other:?}"),
    }
    publisher.cancel();
}

#[test]
fn disabling_auto_publish_stops_future_writes_and_keeps_the_log() {
    let (_fixture, project) = github_project();
    let destination = infer_github_destination(&project).expect("infer destination");
    let mut opportunity =
        Opportunity::verified(approved_draft("missing-rationale:src/auth.rs:1-20@abc123"));
    let mut publisher = eligible_publisher();
    publisher.consider(&candidate(
        &opportunity,
        &destination,
        QualityGrade::APlus,
        false,
    ));
    assert!(publisher.pending().is_some());

    let prior_events = publisher.log().len();
    publisher.disable();
    assert!(!publisher.settings().is_enabled());
    assert!(publisher.pending().is_none());
    assert!(publisher
        .log()
        .iter()
        .any(|event| matches!(event, AutoPublishEvent::Disabled)));
    assert!(publisher.log().len() >= prior_events);

    let blocked = publisher.consider(&candidate(
        &opportunity,
        &destination,
        QualityGrade::APlus,
        false,
    ));
    assert_eq!(
        blocked,
        AutoPublishDecision::Blocked(AutoPublishBlock::Disabled)
    );

    let server = LocalHttpFake::start_with(vec![StubResponse::json(
        500,
        r#"{"error":"disabled auto-publish must not write"}"#,
    )]);
    let client = GitHubClient::new()
        .with_base_url(format!("http://{}", server.address()))
        .with_timeout(Duration::from_secs(2));
    let error = publisher
        .publish_due(&client, &publish_auth(), &mut opportunity)
        .expect_err("disabled publisher must not write");
    assert!(matches!(error, AutoPublishError::NoPendingCountdown));
    assert!(server.requests().is_empty());
    assert_no_live_github(&server);
    assert_eq!(opportunity.status, OpportunityStatus::Verified);
}
