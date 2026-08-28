mod support;

use std::time::Duration;

use subsurface_engine::fixture::GitFixture;
use subsurface_engine::github::{
    fetch_work_item_state, fingerprint_marker, infer_github_destination, publish_work_item_preview,
    GitHubAuth, GitHubAuthMethod, GitHubClient, PublishOutcome, WorkItemDraft,
    WorkItemTrackerState,
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
