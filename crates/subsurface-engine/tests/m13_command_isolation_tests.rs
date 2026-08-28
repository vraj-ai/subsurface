use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use subsurface_engine::candidate::{fingerprint_project, prepare_candidate, CandidateEdit};
use subsurface_engine::command::{
    AllowedCommand, CommandAllowlist, CommandOutcome, IsolatedCommand, IsolatedRunner,
    IsolationError, ResourceBounds,
};
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::project::Project;

mod support;
use support::{LocalHttpFake, StubResponse};

fn prepared_project(
    edits: &[CandidateEdit],
) -> (
    GitFixture,
    subsurface_engine::candidate::PreparedCandidate,
    IsolatedRunner,
) {
    let mut fixture = GitFixture::new();
    fixture.commit("initial", &[("src/lib.rs", "// code\n")]);
    let scratch = fixture.path().join("scratch.txt");
    fs::write(&scratch, "local only\n").expect("write uncommitted file");
    let project = Project::open(fixture.path()).expect("open project");
    let prepared = prepare_candidate(&project, edits).expect("prepare candidate");
    let allowlist = CommandAllowlist::new(vec![
        AllowedCommand::new("echo"),
        AllowedCommand::new("sleep"),
        AllowedCommand::new("yes"),
        AllowedCommand::new("tee"),
        AllowedCommand::new("python3").with_arg_prefix(["dump_env.py"]),
        AllowedCommand::new("python3").with_arg_prefix(["busy.py"]),
        AllowedCommand::new("curl"),
    ]);
    let bounds = ResourceBounds {
        timeout: Duration::from_secs(2),
        max_output_bytes: 64,
        max_cpu: Duration::from_secs(1),
        max_concurrency: 1,
    };
    let runner = IsolatedRunner::new(project, prepared.clone_path.clone(), allowlist, bounds);
    (fixture, prepared, runner)
}

fn runner_from(
    fixture: GitFixture,
    project: Project,
    prepared: subsurface_engine::candidate::PreparedCandidate,
    allowlist: CommandAllowlist,
    bounds: ResourceBounds,
) -> (
    GitFixture,
    subsurface_engine::candidate::PreparedCandidate,
    IsolatedRunner,
) {
    let runner = IsolatedRunner::new(project, prepared.clone_path.clone(), allowlist, bounds);
    (fixture, prepared, runner)
}

#[test]
fn disallowed_command_is_rejected() {
    let (_fixture, _prepared, runner) = prepared_project(&[]);
    let error = runner
        .run(&IsolatedCommand::new("rm", ["-rf", "/"]))
        .expect_err("disallowed command");
    assert!(
        matches!(
            error,
            IsolationError::NotAllowlisted { ref program, .. } if program == "rm"
        ),
        "expected allowlist rejection, got {error:?}"
    );
}

#[test]
fn path_escape_is_rejected() {
    let (_fixture, _prepared, runner) = prepared_project(&[]);
    let error = runner
        .run(&IsolatedCommand::new("../echo", ["escaped"]))
        .expect_err("path escape");
    assert!(
        matches!(error, IsolationError::PathEscape(ref path) if path.contains("..")),
        "expected path escape, got {error:?}"
    );
}

#[test]
fn inherited_secrets_are_scrubbed() {
    let (_fixture, _prepared, runner) = prepared_project(&[CandidateEdit {
        path: "dump_env.py".into(),
        contents: concat!(
            "import os\n",
            "print('GITHUB_TOKEN=' + os.environ.get('GITHUB_TOKEN', 'MISSING'))\n",
            "print('OPENAI_API_KEY=' + os.environ.get('OPENAI_API_KEY', 'MISSING'))\n",
            "print('PATH=' + ('SET' if os.environ.get('PATH') else 'MISSING'))\n",
        )
        .into(),
    }]);
    std::env::set_var("GITHUB_TOKEN", "super-secret-isolation-token");
    std::env::set_var("OPENAI_API_KEY", "sk-isolation-test");
    let receipt = runner
        .run(&IsolatedCommand::new("python3", ["dump_env.py"]))
        .expect("dump env");
    assert_eq!(receipt.outcome, CommandOutcome::Succeeded);
    assert!(
        receipt.stdout.contains("GITHUB_TOKEN=MISSING"),
        "secret leaked: {}",
        receipt.stdout
    );
    assert!(
        receipt.stdout.contains("OPENAI_API_KEY=MISSING"),
        "secret leaked: {}",
        receipt.stdout
    );
    assert!(
        receipt.stdout.contains("PATH=SET"),
        "PATH should survive scrubbing: {}",
        receipt.stdout
    );
}

#[test]
fn network_is_denied_by_default() {
    let server = LocalHttpFake::start_with(vec![StubResponse::json(200, r#"{"ok":true}"#)]);
    let (_fixture, _prepared, runner) = prepared_project(&[]);
    let url = format!("http://{}/probe", server.address());
    let receipt = runner
        .run(&IsolatedCommand::new(
            "curl",
            ["-sS", "--max-time", "2", "--noproxy", "*", &url],
        ))
        .expect("denied curl still yields a receipt");
    assert_ne!(receipt.outcome, CommandOutcome::Succeeded);
    assert!(
        server.requests().is_empty(),
        "denied command must not reach the network, got {:?}",
        server.requests()
    );
}

#[test]
fn network_can_be_granted_per_command() {
    let server = LocalHttpFake::start_with(vec![StubResponse::json(200, r#"{"ok":true}"#)]);
    let mut fixture = GitFixture::new();
    fixture.commit("initial", &[("src/lib.rs", "// code\n")]);
    let project = Project::open(fixture.path()).expect("open project");
    let prepared = prepare_candidate(&project, &[]).expect("prepare");
    let allowlist = CommandAllowlist::new(vec![AllowedCommand::new("curl").with_network(true)]);
    let bounds = ResourceBounds {
        timeout: Duration::from_secs(3),
        max_output_bytes: 4096,
        max_cpu: Duration::from_secs(3),
        max_concurrency: 1,
    };
    let (_fixture, _prepared, runner) = runner_from(fixture, project, prepared, allowlist, bounds);
    let url = format!("http://{}/probe", server.address());
    let receipt = runner
        .run(&IsolatedCommand::new(
            "curl",
            ["-sS", "--max-time", "2", "--noproxy", "*", &url],
        ))
        .expect("granted curl");
    assert_eq!(
        receipt.outcome,
        CommandOutcome::Succeeded,
        "{}",
        receipt.stderr
    );
    assert!(
        !server.requests().is_empty(),
        "granted command should reach the local fake"
    );
}

#[test]
fn timeout_terminates_with_receipt() {
    let (_fixture, _prepared, runner) = prepared_project(&[]);
    let receipt = runner
        .run(&IsolatedCommand::new("sleep", ["10"]))
        .expect("timeout receipt");
    assert_eq!(receipt.outcome, CommandOutcome::TimedOut);
}

#[test]
fn output_limit_terminates_with_receipt() {
    let (_fixture, _prepared, runner) = prepared_project(&[]);
    let receipt = runner
        .run(&IsolatedCommand::new("yes", Vec::<String>::new()))
        .expect("output limit receipt");
    assert_eq!(receipt.outcome, CommandOutcome::OutputLimited);
    assert!(
        receipt.stdout.len() <= 64,
        "stdout exceeded bound: {}",
        receipt.stdout.len()
    );
}

#[test]
fn concurrency_limit_is_enforced() {
    let (_fixture, _prepared, runner) = prepared_project(&[]);
    let started = Arc::new(AtomicUsize::new(0));
    let flag = Arc::clone(&started);
    let sleeper = runner.clone();
    let handle = thread::spawn(move || {
        flag.store(1, Ordering::SeqCst);
        sleeper.run(&IsolatedCommand::new("sleep", ["1"]))
    });
    while started.load(Ordering::SeqCst) == 0 {
        thread::sleep(Duration::from_millis(5));
    }
    thread::sleep(Duration::from_millis(50));
    let error = runner
        .run(&IsolatedCommand::new("echo", ["too-many"]))
        .expect_err("concurrency");
    assert_eq!(error, IsolationError::ConcurrencyLimit);
    let sleeper_receipt = handle
        .join()
        .expect("sleeper thread")
        .expect("sleeper command");
    assert!(
        matches!(
            sleeper_receipt.outcome,
            CommandOutcome::Succeeded | CommandOutcome::TimedOut | CommandOutcome::Failed
        ),
        "sleeper outcome {:?}",
        sleeper_receipt.outcome
    );
}

#[test]
fn cpu_bound_terminates_with_receipt() {
    let (_fixture, _prepared, runner) = prepared_project(&[CandidateEdit {
        path: "busy.py".into(),
        contents: "while True:\n    pass\n".into(),
    }]);
    let receipt = runner
        .run(&IsolatedCommand::new("python3", ["busy.py"]))
        .expect("cpu receipt");
    assert!(
        matches!(
            receipt.outcome,
            CommandOutcome::CpuLimited | CommandOutcome::TimedOut
        ),
        "expected CPU or timeout bound, got {:?}",
        receipt.outcome
    );
}

#[test]
fn active_project_hash_unchanged_after_isolated_commands() {
    let (fixture, _prepared, runner) = prepared_project(&[]);
    let project = Project::open(fixture.path()).expect("reopen project");
    let before = fingerprint_project(&project).expect("fingerprint before");
    assert!(before.porcelain.contains("scratch.txt"));

    let ok = runner
        .run(&IsolatedCommand::new("echo", ["isolated-ok"]))
        .expect("echo");
    assert_eq!(ok.outcome, CommandOutcome::Succeeded);
    assert!(ok.stdout.contains("isolated-ok"));
    assert_eq!(ok.fingerprint_before, before);
    assert_eq!(ok.fingerprint_after, before);

    let failed = runner
        .run(&IsolatedCommand::new(
            "tee",
            [project
                .root_path
                .join("pwned.txt")
                .to_string_lossy()
                .into_owned()],
        ))
        .expect("escaped write is terminated, not applied");
    assert_ne!(failed.outcome, CommandOutcome::Succeeded);

    let after_failure = fingerprint_project(&project).expect("fingerprint after failure");
    assert_eq!(after_failure.head, before.head);
    assert_eq!(after_failure.porcelain, before.porcelain);
    assert_eq!(
        fs::read_to_string(project.root_path.join("src/lib.rs")).expect("project file"),
        "// code\n"
    );
    assert_eq!(
        fs::read_to_string(project.root_path.join("scratch.txt")).expect("uncommitted file"),
        "local only\n"
    );
    assert!(
        !project.root_path.join("pwned.txt").exists(),
        "isolated command must not write into the active Project"
    );
}
