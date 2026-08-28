use std::fs;
use std::time::Duration;

use subsurface_engine::candidate::{fingerprint_project, CandidateEdit};
use subsurface_engine::command::{
    AllowedCommand, CommandAllowlist, CommandOutcome, IsolatedCommand, ResourceBounds,
};
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::project::Project;
use subsurface_engine::receipt::{
    compare_baseline_and_candidate, Check, ImprovementQueue, QueueStatus, ReceiptVerdict,
};

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

fn open_project(lib_rs: &str) -> (GitFixture, Project) {
    let mut fixture = GitFixture::new();
    fixture.commit(
        "initial",
        &[("src/lib.rs", lib_rs), ("check.py", check_script())],
    );
    let scratch = fixture.path().join("scratch.txt");
    fs::write(&scratch, "local only\n").expect("write uncommitted file");
    let project = Project::open(fixture.path()).expect("open project");
    (fixture, project)
}

#[test]
fn baseline_then_candidate_emits_improvement_receipt() {
    let (_fixture, project) = open_project("fn broken() {}\n");
    let before = fingerprint_project(&project).expect("fingerprint before");

    let mut queue = ImprovementQueue::new();
    queue.enqueue("opp-improve");

    let receipt = queue
        .compare(
            "opp-improve",
            &project,
            &[CandidateEdit {
                path: "src/lib.rs".into(),
                contents: "fn improved() {}\n".into(),
            }],
            &[locked_check()],
            allowlist(),
            bounds(),
        )
        .expect("compare baseline and candidate");

    assert_eq!(receipt.verdict, ReceiptVerdict::Improved);
    assert_eq!(receipt.base_commit, before.head);
    assert!(
        receipt
            .improved
            .iter()
            .any(|item| item.contains("locked-checks")),
        "receipt should name what improved, got {:?}",
        receipt.improved
    );
    assert_eq!(receipt.proving_checks, vec!["locked-checks".to_string()]);
    assert!(
        receipt.remaining.is_empty(),
        "successful candidate should not leave remaining failures, got {:?}",
        receipt.remaining
    );
    assert_eq!(receipt.comparisons.len(), 1);
    assert!(!receipt.comparisons[0].baseline.proved);
    assert_eq!(
        receipt.comparisons[0].baseline.outcome,
        CommandOutcome::Failed
    );
    assert!(receipt.comparisons[0].candidate.proved);
    assert_eq!(
        receipt.comparisons[0].candidate.outcome,
        CommandOutcome::Succeeded
    );

    let item = queue.get("opp-improve").expect("queued item");
    assert_eq!(item.status, QueueStatus::Verified);
    assert!(queue.is_queued("opp-improve"));
    assert!(!queue.is_resolved("opp-improve"));

    let after = fingerprint_project(&project).expect("fingerprint after");
    assert_eq!(after, before);
    assert_eq!(
        fs::read_to_string(project.root_path.join("src/lib.rs")).expect("read project file"),
        "fn broken() {}\n"
    );
    assert_eq!(
        fs::read_to_string(project.root_path.join("scratch.txt")).expect("read uncommitted file"),
        "local only\n"
    );
}

#[test]
fn failed_candidate_stays_queued() {
    let (_fixture, project) = open_project("fn broken() {}\n");
    let before = fingerprint_project(&project).expect("fingerprint before");

    let mut queue = ImprovementQueue::new();
    queue.enqueue("opp-failed");
    assert_eq!(
        queue.get("opp-failed").expect("queued").status,
        QueueStatus::Queued
    );

    let receipt = queue
        .compare(
            "opp-failed",
            &project,
            &[CandidateEdit {
                path: "src/lib.rs".into(),
                contents: "fn still_broken() {}\n".into(),
            }],
            &[locked_check()],
            allowlist(),
            bounds(),
        )
        .expect("failed comparison still yields a receipt");

    assert_eq!(receipt.verdict, ReceiptVerdict::Failed);
    assert!(
        receipt.improved.is_empty(),
        "failed candidate should not claim an improvement, got {:?}",
        receipt.improved
    );
    assert!(
        receipt
            .remaining
            .iter()
            .any(|item| item.contains("locked-checks")),
        "receipt should name remaining failures, got {:?}",
        receipt.remaining
    );
    assert!(receipt.proving_checks.is_empty());
    assert!(!receipt.comparisons[0].baseline.proved);
    assert!(!receipt.comparisons[0].candidate.proved);

    let item = queue.get("opp-failed").expect("failed item stays queued");
    assert_eq!(item.status, QueueStatus::Failed);
    assert!(queue.is_queued("opp-failed"));
    assert!(
        !queue.is_resolved("opp-failed"),
        "Failed must stay queued rather than resolving the active Project"
    );
    assert_eq!(
        item.receipt.as_ref().map(|item| item.verdict),
        Some(ReceiptVerdict::Failed)
    );

    let after = fingerprint_project(&project).expect("fingerprint after failure");
    assert_eq!(after, before);
    assert_eq!(
        fs::read_to_string(project.root_path.join("src/lib.rs")).expect("read project file"),
        "fn broken() {}\n"
    );
    assert_eq!(
        fs::read_to_string(project.root_path.join("scratch.txt")).expect("read uncommitted file"),
        "local only\n"
    );
}

#[test]
fn compare_without_queue_leaves_project_untouched() {
    let (_fixture, project) = open_project("fn broken() {}\n");
    let before = fingerprint_project(&project).expect("fingerprint before");

    let receipt = compare_baseline_and_candidate(
        &project,
        &[CandidateEdit {
            path: "src/lib.rs".into(),
            contents: "fn improved() {}\n".into(),
        }],
        &[locked_check()],
        allowlist(),
        bounds(),
    )
    .expect("direct comparison");

    assert_eq!(receipt.verdict, ReceiptVerdict::Improved);
    let after = fingerprint_project(&project).expect("fingerprint after");
    assert_eq!(after, before);
    assert_eq!(receipt.project_fingerprint, before);
}
