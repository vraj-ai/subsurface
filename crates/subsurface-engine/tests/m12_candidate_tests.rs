use std::fs;
use std::os::unix::fs::PermissionsExt;

use subsurface_engine::candidate::{
    fingerprint_project, prepare_candidate, prepare_candidate_with, CandidateEdit, CandidateError,
};
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::project::Project;
use subsurface_engine::runtime::{prepare_via_runtime, OpenCodeRuntime, PrepareOutcome};

#[test]
fn active_project_hash_unchanged_after_preparation() {
    let mut fixture = GitFixture::new();
    fixture.commit("initial", &[("src/lib.rs", "// code\n")]);
    let project = Project::open(fixture.path()).expect("open project");

    let scratch = project.root_path.join("scratch.txt");
    fs::write(&scratch, "local only\n").expect("write uncommitted file");

    let before = fingerprint_project(&project).expect("fingerprint before");
    assert_eq!(before.head.len(), 40);
    assert!(
        before.porcelain.contains("scratch.txt"),
        "expected dirty porcelain before preparation, got {:?}",
        before.porcelain
    );

    let prepared = prepare_candidate(
        &project,
        &[CandidateEdit {
            path: "src/lib.rs".into(),
            contents: "fn improved() {}\n".into(),
        }],
    )
    .expect("prepare candidate");

    let after_success = fingerprint_project(&project).expect("fingerprint after success");
    assert_eq!(after_success.head, before.head);
    assert_eq!(after_success.porcelain, before.porcelain);
    assert_ne!(prepared.clone_path, project.root_path);
    assert_eq!(prepared.base_commit, before.head);
    assert_eq!(
        fs::read_to_string(project.root_path.join("src/lib.rs")).expect("read project file"),
        "// code\n"
    );
    assert_eq!(
        fs::read_to_string(prepared.clone_path.join("src/lib.rs")).expect("read clone file"),
        "fn improved() {}\n"
    );

    drop(prepared);

    let failed = prepare_candidate_with(&project, |clone_root| {
        fs::write(clone_root.join("src/lib.rs"), "fn boom() {}\n")
            .map_err(|error| CandidateError::PreparationFailed(error.to_string()))?;
        Err(CandidateError::PreparationFailed(
            "candidate checks failed".into(),
        ))
    });
    assert!(
        matches!(failed, Err(CandidateError::PreparationFailed(ref reason)) if reason.contains("failed")),
        "expected preparation failure, got {failed:?}"
    );

    let after_failure = fingerprint_project(&project).expect("fingerprint after failure");
    assert_eq!(after_failure.head, before.head);
    assert_eq!(after_failure.porcelain, before.porcelain);
    assert_eq!(
        fs::read_to_string(project.root_path.join("src/lib.rs")).expect("read project file"),
        "// code\n"
    );
    assert_eq!(
        fs::read_to_string(&scratch).expect("read uncommitted file"),
        "local only\n"
    );
}

#[test]
fn no_runtime_yields_actionable_unavailable_state() {
    let mut fixture = GitFixture::new();
    fixture.commit("initial", &[("src/lib.rs", "// code\n")]);
    let project = Project::open(fixture.path()).expect("open project");

    let scratch = project.root_path.join("scratch.txt");
    fs::write(&scratch, "local only\n").expect("write uncommitted file");

    let before = fingerprint_project(&project).expect("fingerprint before");

    let outcome = prepare_via_runtime(&project, &OpenCodeRuntime::none(), "improve src/lib.rs")
        .expect("missing runtime is an unavailable state, not an error");

    let PrepareOutcome::Unavailable(state) = outcome else {
        panic!("expected unavailable state, got {outcome:?}");
    };

    let message = state.message();
    let lowered = message.to_ascii_lowercase();
    assert!(
        lowered.contains("opencode") && lowered.contains("unavailable"),
        "state should name the missing runtime, got {message:?}"
    );
    assert!(
        !state.next_verb.is_empty(),
        "unavailable state must name a next verb"
    );
    assert!(
        lowered.contains("install") || lowered.contains("connect") || lowered.contains("configure"),
        "state should name the next verb, got {message:?}"
    );

    let after = fingerprint_project(&project).expect("fingerprint after unavailable prepare");
    assert_eq!(after.head, before.head);
    assert_eq!(after.porcelain, before.porcelain);
    assert_eq!(
        fs::read_to_string(project.root_path.join("src/lib.rs")).expect("read project file"),
        "// code\n"
    );
    assert_eq!(
        fs::read_to_string(&scratch).expect("read uncommitted file"),
        "local only\n"
    );
}

#[test]
fn configured_runtime_prepares_inside_disposable_clone() {
    let mut fixture = GitFixture::new();
    fixture.commit("initial", &[("src/lib.rs", "// code\n")]);
    let project = Project::open(fixture.path()).expect("open project");

    let scratch = project.root_path.join("scratch.txt");
    fs::write(&scratch, "local only\n").expect("write uncommitted file");
    let before = fingerprint_project(&project).expect("fingerprint before");

    let temp = tempfile::tempdir().expect("tempdir");
    let executable = temp.path().join("opencode");
    fs::write(
        &executable,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 1.2.3; exit 0; fi\nprintf 'fn improved() {}\\n' > src/lib.rs\necho prepared\n",
    )
    .expect("write OpenCode fake");
    let mut permissions = fs::metadata(&executable).expect("metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&executable, permissions).expect("make OpenCode fake executable");

    let runtime = OpenCodeRuntime::from_executable(&executable).expect("detect fake runtime");
    let outcome = prepare_via_runtime(&project, &runtime, "improve src/lib.rs")
        .expect("configured runtime should prepare");

    let PrepareOutcome::Prepared(prepared) = outcome else {
        panic!("expected prepared candidate, got {outcome:?}");
    };

    assert_ne!(prepared.clone_path, project.root_path);
    assert_eq!(prepared.base_commit, before.head);
    assert_eq!(
        fs::read_to_string(prepared.clone_path.join("src/lib.rs")).expect("read clone file"),
        "fn improved() {}\n"
    );
    assert_eq!(
        fs::read_to_string(project.root_path.join("src/lib.rs")).expect("read project file"),
        "// code\n"
    );

    let after = fingerprint_project(&project).expect("fingerprint after runtime prepare");
    assert_eq!(after.head, before.head);
    assert_eq!(after.porcelain, before.porcelain);
    assert_eq!(
        fs::read_to_string(&scratch).expect("read uncommitted file"),
        "local only\n"
    );
}
