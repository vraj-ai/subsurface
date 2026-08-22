use subsurface_engine::budget::{rank_and_budget_evidence, BudgetConfig};
use subsurface_engine::confidence::{assign_confidence, Confidence};
use subsurface_engine::evidence::{walk_evidence, LineRange};
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::site::Site;

#[test]
fn test_evidence_walk_rename_and_move() {
    let mut fixture = GitFixture::new();
    let c1 = fixture.commit(
        "initial file",
        &[(
            "src/old_calc.rs",
            "fn add(a: i32, b: i32) -> i32 {\n    // line 2\n    let result = a + b;\n    result\n}\n",
        )],
    );

    let c2 = fixture.commit(
        "update result logic",
        &[(
            "src/old_calc.rs",
            "fn add(a: i32, b: i32) -> i32 {\n    // line 2\n    let result = a.saturating_add(b);\n    result\n}\n",
        )],
    );

    let _c3 = fixture.rename_file(
        "src/old_calc.rs",
        "src/new_calc.rs",
        "rename calc to new_calc",
    );

    let c4 = fixture.commit(
        "add logging comment",
        &[(
            "src/new_calc.rs",
            "fn add(a: i32, b: i32) -> i32 {\n    // line 2 updated\n    let result = a.saturating_add(b);\n    result\n}\n",
        )],
    );

    let site = Site::open(fixture.path()).expect("open site");
    let range = LineRange { start: 2, end: 4 };
    let evidence = walk_evidence(&site, "src/new_calc.rs", range).expect("walk evidence");

    // Must find commits touching the range across the rename
    assert!(!evidence.is_empty(), "evidence should not be empty");
    let shas: Vec<&str> = evidence.iter().map(|e| e.commit_sha.as_str()).collect();
    assert!(shas.contains(&c4.as_str()) || shas.contains(&c2.as_str()) || shas.contains(&c1.as_str()));
}

#[test]
fn test_evidence_walk_reformat_and_whitespace() {
    let mut fixture = GitFixture::new();
    fixture.commit(
        "initial",
        &[(
            "src/worker.rs",
            "pub fn run() {\nlet x = 1;\nlet y = 2;\n}\n",
        )],
    );

    fixture.commit(
        "reformat code with indent",
        &[(
            "src/worker.rs",
            "pub fn run() {\n    let x = 1;\n    let y = 2;\n}\n",
        )],
    );

    let c3 = fixture.commit(
        "update y value",
        &[(
            "src/worker.rs",
            "pub fn run() {\n    let x = 1;\n    let y = 42;\n}\n",
        )],
    );

    let site = Site::open(fixture.path()).expect("open site");
    let range = LineRange { start: 2, end: 3 };
    let evidence = walk_evidence(&site, "src/worker.rs", range).expect("walk evidence");

    assert!(!evidence.is_empty());
    let shas: Vec<&str> = evidence.iter().map(|e| e.commit_sha.as_str()).collect();
    assert!(shas.contains(&c3.as_str()));
}

#[test]
fn test_evidence_walk_merge_commits() {
    let mut fixture = GitFixture::new();
    fixture.commit("base", &[("src/lib.rs", "fn foo() {\n    1\n}\n")]);
    fixture.branch("feature");
    fixture.checkout("feature");
    let feat_sha = fixture.commit(
        "feature change",
        &[("src/lib.rs", "fn foo() {\n    // changed in feature\n    2\n}\n")],
    );
    fixture.checkout("main");
    let merge_sha = fixture.merge_branch("feature", "Merge branch feature");

    let site = Site::open(fixture.path()).expect("open site");
    let range = LineRange { start: 1, end: 3 };
    let evidence = walk_evidence(&site, "src/lib.rs", range).expect("walk evidence");

    let shas: Vec<&str> = evidence.iter().map(|e| e.commit_sha.as_str()).collect();
    assert!(shas.contains(&feat_sha.as_str()) || shas.contains(&merge_sha.as_str()));
}

#[test]
fn test_link_cocommitted_tests_and_docs() {
    let mut fixture = GitFixture::new();
    let sha = fixture.commit(
        "Add auth guard with tests and documentation",
        &[
            ("src/auth.rs", "pub fn verify() -> bool { true }\n"),
            ("tests/auth_test.rs", "// test auth\n#[test]\nfn test_auth() {}\n"),
            ("docs/auth.md", "# Auth specification\nExplaining auth flow.\n"),
        ],
    );

    let site = Site::open(fixture.path()).expect("open site");
    let range = LineRange { start: 1, end: 1 };
    let evidence = walk_evidence(&site, "src/auth.rs", range).expect("walk evidence");

    let cocommitted_tests: Vec<_> = evidence.iter().filter(|e| e.is_heuristic && e.file_path.contains("tests/")).collect();
    let cocommitted_docs: Vec<_> = evidence.iter().filter(|e| e.is_heuristic && e.file_path.contains("docs/")).collect();

    assert!(!cocommitted_tests.is_empty(), "Should link test file as heuristic evidence");
    assert!(!cocommitted_docs.is_empty(), "Should link doc file as heuristic evidence");
    assert!(cocommitted_tests[0].heuristic_note.is_some());
    assert_eq!(cocommitted_tests[0].commit_sha, sha);
}

#[test]
fn test_evidence_budget_ranking_and_exclusion() {
    let mut fixture = GitFixture::new();
    let mut shas = Vec::new();
    for i in 1..=12 {
        let content = format!("fn func() {{\n    let x = {};\n    let y = {};\n}}\n", i, i * 2);
        let msg = format!("commit {}", i);
        let sha = fixture.commit(&msg, &[("src/mod.rs", &content)]);
        shas.push(sha);
    }

    let site = Site::open(fixture.path()).expect("open site");
    let range = LineRange { start: 1, end: 3 };
    let candidate_evidence = walk_evidence(&site, "src/mod.rs", range).expect("walk evidence");

    let budget_config = BudgetConfig { max_evidence_items: 4 };
    let budget_result = rank_and_budget_evidence(&candidate_evidence, &budget_config);

    assert_eq!(budget_result.included.len(), 4, "Should cap at max budget items");
    assert!(
        budget_result.excluded.len() >= candidate_evidence.len() - 4,
        "Excluded items must be recorded explicitly"
    );

    // Invariant: Excluded evidence has SHA and non-empty exclusion reason
    for excluded in &budget_result.excluded {
        assert!(!excluded.commit_sha.is_empty());
        assert!(!excluded.reason.is_empty());
    }
}

#[test]
fn test_confidence_rule_stated() {
    let mut fixture = GitFixture::new();
    fixture.commit(
        "Workaround for upstream issue #42: socket reset under high load because buffer overflow",
        &[("src/net.rs", "fn reconnect() { /* buffer fix */ }\n")],
    );

    let site = Site::open(fixture.path()).expect("open site");
    let range = LineRange { start: 1, end: 1 };
    let evidence = walk_evidence(&site, "src/net.rs", range).expect("walk evidence");

    let confidence = assign_confidence(&evidence);
    assert_eq!(confidence, Confidence::Stated);
}

#[test]
fn test_confidence_rule_inferred() {
    let mut fixture = GitFixture::new();
    // Commit message without explicit explanation sentences, but with co-committed test and config
    fixture.commit(
        "update auth handler",
        &[
            ("src/auth.rs", "fn check() { /* updated */ }\n"),
            ("tests/auth_test.rs", "// test auth\n"),
        ],
    );

    let site = Site::open(fixture.path()).expect("open site");
    let range = LineRange { start: 1, end: 1 };
    let evidence = walk_evidence(&site, "src/auth.rs", range).expect("walk evidence");

    let confidence = assign_confidence(&evidence);
    assert_eq!(confidence, Confidence::Inferred);
}

#[test]
fn test_confidence_rule_none() {
    let mut fixture = GitFixture::new();
    // Commit messages all "fix" / "wip" with no rationale, no tests, no docs
    fixture.commit("fix", &[("src/misc.rs", "fn run() {}\n")]);
    fixture.commit("wip", &[("src/misc.rs", "fn run() { let a = 1; }\n")]);
    fixture.commit("fix typo", &[("src/misc.rs", "fn run() { let a = 2; }\n")]);

    let site = Site::open(fixture.path()).expect("open site");
    let range = LineRange { start: 1, end: 1 };
    let evidence = walk_evidence(&site, "src/misc.rs", range).expect("walk evidence");

    let confidence = assign_confidence(&evidence);
    assert_eq!(confidence, Confidence::None);
}
