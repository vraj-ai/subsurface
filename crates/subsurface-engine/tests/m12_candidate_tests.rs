use std::fs;

use subsurface_engine::candidate::{
    fingerprint_project, prepare_candidate, prepare_candidate_with, CandidateEdit, CandidateError,
};
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::project::Project;

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
