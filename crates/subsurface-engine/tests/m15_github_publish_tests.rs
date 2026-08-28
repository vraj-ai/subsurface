mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use subsurface_engine::fixture::GitFixture;
use subsurface_engine::github::{
    fingerprint_marker, github_oauth_client, infer_github_destination, publish_work_item,
    render_work_item, resolve_github_auth, resolve_github_publish_target, work_item_fingerprint,
    GhCli, GhPermission, GitHubAuth, GitHubAuthMethod, GitHubAuthProbes, GitHubClient, GitHubError,
    PublishOutcome, WorkItemDestination, WorkItemDraft,
};
use subsurface_engine::oauth::OAuthClient;
use subsurface_engine::project::Project;
use subsurface_engine::receipt::{ImprovementReceipt, ReceiptVerdict};
use support::{LocalHttpFake, StubResponse};

fn fake_gh(
    dir: &std::path::Path,
    log_path: &std::path::Path,
    token: Option<&str>,
) -> std::path::PathBuf {
    let executable = dir.join("gh");
    let token_line = match token {
        Some(value) => format!("echo {value}\nexit 0\n"),
        None => "echo 'gh not authenticated' >&2\nexit 1\n".to_string(),
    };
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{}\"\nif [ \"$1\" = \"auth\" ] && [ \"$2\" = \"token\" ]; then\n{token_line}fi\necho 'unexpected gh invocation' >&2\nexit 1\n",
            log_path.display(),
        ),
    )
    .expect("write fake gh");
    let mut permissions = fs::metadata(&executable).expect("metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&executable, permissions).expect("make fake gh executable");
    executable
}

fn gh_invocations(log_path: &std::path::Path) -> Vec<String> {
    if !log_path.exists() {
        return Vec::new();
    }
    fs::read_to_string(log_path)
        .expect("read gh log")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn browser_token_response() -> StubResponse {
    StubResponse::json(
        200,
        r#"{"access_token":"browser-token","refresh_token":"browser-refresh","expires_in":3600,"token_type":"Bearer"}"#,
    )
}

fn denied_token_response() -> StubResponse {
    StubResponse::json(400, r#"{"error":"access_denied"}"#)
}

fn device_start_response() -> StubResponse {
    StubResponse::json(
        200,
        r#"{"device_code":"device-secret","user_code":"ABCD-EFGH","verification_uri":"http://127.0.0.1/device","expires_in":600,"interval":1}"#,
    )
}

fn device_token_response() -> StubResponse {
    StubResponse::json(
        200,
        r#"{"access_token":"device-token","refresh_token":"device-refresh","expires_in":3600,"token_type":"Bearer"}"#,
    )
}

fn oauth_for(server: &LocalHttpFake) -> OAuthClient {
    let base = format!("http://{}", server.address());
    github_oauth_client(
        "subsurface-test",
        format!("{base}/authorize"),
        format!("{base}/token"),
        format!("{base}/device/code"),
    )
    .with_timeout(Duration::from_secs(1))
}

#[test]
fn github_auth_and_destination_fallback_order() {
    assert_eq!(WorkItemDestination::ONLY, WorkItemDestination::GitHub);
    assert_eq!(
        GitHubAuthMethod::FALLBACK_ORDER,
        [
            GitHubAuthMethod::GhCli,
            GitHubAuthMethod::Browser,
            GitHubAuthMethod::Device,
            GitHubAuthMethod::Token,
        ]
    );

    let mut github_project = GitFixture::new();
    github_project.commit("initial", &[("README.md", "widgets\n")]);
    github_project.add_remote("gitlab", "https://gitlab.com/acme/widgets.git");
    github_project.add_remote("origin", "git@github.com:acme/widgets.git");
    github_project.add_remote("upstream", "https://github.com/acme/widgets-upstream.git");
    let project = Project::open(github_project.path()).expect("open github project");
    let destination = infer_github_destination(&project).expect("infer GitHub destination");
    assert_eq!(destination.kind(), WorkItemDestination::GitHub);
    assert_eq!(destination.owner, "acme");
    assert_eq!(destination.repo, "widgets");
    assert_eq!(destination.remote_name, "origin");
    assert_eq!(destination.remote_url, "git@github.com:acme/widgets.git");

    let mut gitlab_only = GitFixture::new();
    gitlab_only.commit("initial", &[("README.md", "elsewhere\n")]);
    gitlab_only.add_remote("origin", "https://gitlab.com/acme/widgets.git");
    let gitlab_project = Project::open(gitlab_only.path()).expect("open gitlab project");
    assert!(matches!(
        infer_github_destination(&gitlab_project),
        Err(GitHubError::NotGitHub)
    ));

    let mut no_remotes = GitFixture::new();
    no_remotes.commit("initial", &[("README.md", "local\n")]);
    let local_project = Project::open(no_remotes.path()).expect("open local project");
    assert!(matches!(
        infer_github_destination(&local_project),
        Err(GitHubError::NoRemote)
    ));

    let gh_dir = tempfile::tempdir().expect("gh dir");
    let gh_log = gh_dir.path().join("invocations.log");
    let succeeding_gh = fake_gh(gh_dir.path(), &gh_log, Some("gho_fake_token"));

    let browser_server = LocalHttpFake::start_with(vec![browser_token_response()]);
    let browser_probes = GitHubAuthProbes::new()
        .with_gh(GhCli::from_executable(&succeeding_gh).expect("gh cli"))
        .with_oauth(oauth_for(&browser_server), "http://127.0.0.1:8787/callback")
        .with_browser_code("browser-code")
        .with_token("pasted-token");

    let without_permission =
        resolve_github_auth(GhPermission::denied(), &browser_probes).expect("browser fallback");
    assert_eq!(without_permission.method, GitHubAuthMethod::Browser);
    assert_eq!(
        without_permission.attempted,
        vec![GitHubAuthMethod::Browser]
    );
    assert_eq!(without_permission.token, "browser-token");
    assert!(
        gh_invocations(&gh_log).is_empty(),
        "gh must not run before permission is granted: {:?}",
        gh_invocations(&gh_log)
    );
    assert_eq!(browser_server.requests().len(), 1);
    assert!(browser_server.requests()[0].contains("grant_type=authorization_code"));

    let with_permission =
        resolve_github_auth(GhPermission::granted(), &browser_probes).expect("gh auth");
    assert_eq!(with_permission.method, GitHubAuthMethod::GhCli);
    assert_eq!(with_permission.attempted, vec![GitHubAuthMethod::GhCli]);
    assert_eq!(with_permission.token, "gho_fake_token");
    assert_eq!(gh_invocations(&gh_log), vec!["auth token".to_string()]);
    assert_eq!(
        browser_server.requests().len(),
        1,
        "successful gh auth must not continue into OAuth"
    );

    let (resolved_destination, resolved_auth) = resolve_github_publish_target(
        &project,
        GhPermission::granted(),
        &GitHubAuthProbes::new()
            .with_gh(GhCli::from_executable(&succeeding_gh).expect("gh cli"))
            .with_token("unused-token"),
    )
    .expect("resolve publish target");
    assert_eq!(resolved_destination.owner, "acme");
    assert_eq!(resolved_destination.repo, "widgets");
    assert_eq!(resolved_auth.method, GitHubAuthMethod::GhCli);

    let fail_dir = tempfile::tempdir().expect("failing gh dir");
    let fail_log = fail_dir.path().join("invocations.log");
    let failing_gh = fake_gh(fail_dir.path(), &fail_log, None);

    let device_server = LocalHttpFake::start_with(vec![
        denied_token_response(),
        device_start_response(),
        device_token_response(),
    ]);
    let device_probes = GitHubAuthProbes::new()
        .with_gh(GhCli::from_executable(&failing_gh).expect("failing gh"))
        .with_oauth(oauth_for(&device_server), "http://127.0.0.1:8787/callback")
        .with_browser_code("rejected-code")
        .with_token("pasted-token");
    let device_auth =
        resolve_github_auth(GhPermission::granted(), &device_probes).expect("device fallback");
    assert_eq!(device_auth.method, GitHubAuthMethod::Device);
    assert_eq!(
        device_auth.attempted,
        vec![
            GitHubAuthMethod::GhCli,
            GitHubAuthMethod::Browser,
            GitHubAuthMethod::Device,
        ]
    );
    assert_eq!(device_auth.token, "device-token");
    assert_eq!(gh_invocations(&fail_log), vec!["auth token".to_string()]);
    let device_requests = device_server.requests();
    assert_eq!(device_requests.len(), 3);
    assert!(device_requests[0].contains("grant_type=authorization_code"));
    assert!(device_requests[1].starts_with("POST /device/code HTTP/1.1\r\n"));
    assert!(device_requests[2]
        .contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"));
    assert!(
        !device_requests
            .iter()
            .any(|request| request.contains("api.github.com")),
        "tests must not write to live GitHub"
    );

    let token_dir = tempfile::tempdir().expect("token gh dir");
    let token_log = token_dir.path().join("invocations.log");
    let token_gh = fake_gh(token_dir.path(), &token_log, None);
    let token_server = LocalHttpFake::start_with(vec![
        denied_token_response(),
        StubResponse::json(400, r#"{"error":"unsupported"}"#),
    ]);
    let token_probes = GitHubAuthProbes::new()
        .with_gh(GhCli::from_executable(&token_gh).expect("token gh"))
        .with_oauth(oauth_for(&token_server), "http://127.0.0.1:8787/callback")
        .with_browser_code("rejected-code")
        .with_token("pasted-token");
    let token_auth =
        resolve_github_auth(GhPermission::granted(), &token_probes).expect("token fallback");
    assert_eq!(token_auth.method, GitHubAuthMethod::Token);
    assert_eq!(
        token_auth.attempted,
        vec![
            GitHubAuthMethod::GhCli,
            GitHubAuthMethod::Browser,
            GitHubAuthMethod::Device,
            GitHubAuthMethod::Token,
        ]
    );
    assert_eq!(token_auth.token, "pasted-token");
    assert!(token_server.address().ip().is_loopback());
}

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

fn search_hit(number: u64, body: &str) -> StubResponse {
    let escaped = body
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    StubResponse::json(
        200,
        format!(
            r#"{{"total_count":1,"items":[{{"number":{number},"html_url":"http://127.0.0.1/issues/{number}","title":"Restore rationale for auth","body":"{escaped}"}}]}}"#
        ),
    )
}

fn created_issue(number: u64, body: &str) -> StubResponse {
    let escaped = body
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    StubResponse::json(
        201,
        format!(
            r#"{{"number":{number},"html_url":"http://127.0.0.1/issues/{number}","title":"Restore rationale for auth","body":"{escaped}"}}"#
        ),
    )
}

fn request_is_create(request: &str) -> bool {
    request.starts_with("POST /repos/acme/widgets/issues ")
}

fn request_is_search(request: &str) -> bool {
    request.starts_with("GET /search/issues?")
}

#[test]
fn publishes_fingerprinted_work_item_body() {
    let (_fixture, project) = github_project();
    let destination = infer_github_destination(&project).expect("infer destination");
    let draft = approved_draft("missing-rationale:src/auth.rs:1-20@abc123");
    let rendered = render_work_item(&destination, &draft);
    let same_fingerprint = work_item_fingerprint(&destination, &draft);
    assert_eq!(rendered.fingerprint, same_fingerprint);
    assert_eq!(
        work_item_fingerprint(&destination, &approved_draft(&draft.id)),
        rendered.fingerprint,
        "fingerprint must be stable for the same Work Item identity"
    );
    let mut other = approved_draft("other-id");
    other.file_path = "src/other.rs".into();
    assert_ne!(
        work_item_fingerprint(&destination, &other),
        rendered.fingerprint
    );

    assert!(
        rendered.body.contains("Work Item"),
        "published body must name a Work Item"
    );
    assert!(rendered.body.contains("## Evidence"));
    assert!(rendered.body.contains("## Improvement Receipt"));
    assert!(rendered.body.contains("`src/auth.rs:1-20@abc123`"));
    assert!(rendered.body.contains("missing-rationale"));
    assert!(rendered
        .body
        .contains(&fingerprint_marker(&rendered.fingerprint)));
    assert!(
        !rendered.body.contains("Opportunity"),
        "published body must not call the Work Item an Opportunity: {}",
        rendered.body
    );
    assert!(
        !rendered.body.contains("Finding"),
        "published body must not call the Work Item a Finding: {}",
        rendered.body
    );

    let server = LocalHttpFake::start_with(vec![empty_search(), created_issue(81, &rendered.body)]);
    let client = GitHubClient::new()
        .with_base_url(format!("http://{}", server.address()))
        .with_timeout(Duration::from_secs(2));
    let published = publish_work_item(&client, &destination, &publish_auth(), &draft)
        .expect("publish Work Item");
    assert_eq!(published.number, 81);
    assert_eq!(published.outcome, PublishOutcome::Created);
    assert_eq!(published.fingerprint, rendered.fingerprint);
    assert_eq!(published.destination.owner, "acme");
    assert_eq!(published.destination.repo, "widgets");
    assert_eq!(published.destination.kind(), WorkItemDestination::GitHub);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(request_is_search(&requests[0]));
    assert!(requests[0].contains(&rendered.fingerprint));
    assert!(request_is_create(&requests[1]));
    assert!(
        requests[1].contains(&fingerprint_marker(&rendered.fingerprint)),
        "create body must embed the fingerprint"
    );
    assert!(requests[1].contains("This Work Item is an approved change proposal"));
    assert!(
        requests[1].to_ascii_lowercase().contains("authorization:")
            && requests[1].contains("gho_test_token"),
        "create must send the bearer token; headers were:\n{}",
        requests[1].split("\r\n\r\n").next().unwrap_or(&requests[1])
    );
    assert!(
        !requests
            .iter()
            .any(|request| request.contains("api.github.com")),
        "tests must not write to live GitHub"
    );
    assert!(server.address().ip().is_loopback());
}

#[test]
fn republish_finds_existing_fingerprint_without_duplicate() {
    let (_fixture, project) = github_project();
    let destination = infer_github_destination(&project).expect("infer destination");
    let draft = approved_draft("missing-rationale:src/auth.rs:1-20@abc123");
    let rendered = render_work_item(&destination, &draft);
    let server = LocalHttpFake::start_with(vec![
        search_hit(81, &rendered.body),
        search_hit(81, &rendered.body),
        StubResponse::json(500, r#"{"error":"duplicate create must not run"}"#),
    ]);
    let client = GitHubClient::new()
        .with_base_url(format!("http://{}", server.address()))
        .with_timeout(Duration::from_secs(2));

    let first = publish_work_item(&client, &destination, &publish_auth(), &draft)
        .expect("first publish recovers existing");
    let second = publish_work_item(&client, &destination, &publish_auth(), &draft)
        .expect("second publish is idempotent");
    assert_eq!(first.number, 81);
    assert_eq!(second.number, 81);
    assert_eq!(first.outcome, PublishOutcome::Existing);
    assert_eq!(second.outcome, PublishOutcome::Existing);
    assert_eq!(first.fingerprint, rendered.fingerprint);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| request_is_search(request)));
    assert!(
        !requests.iter().any(|request| request_is_create(request)),
        "existing fingerprint must not create a duplicate issue: {requests:?}"
    );
}

#[test]
fn timeout_after_success_recovers_existing_fingerprint() {
    let (_fixture, project) = github_project();
    let destination = infer_github_destination(&project).expect("infer destination");
    let draft = approved_draft("missing-rationale:src/auth.rs:1-20@abc123");
    let rendered = render_work_item(&destination, &draft);
    let server = LocalHttpFake::start_with(vec![
        empty_search(),
        created_issue(81, &rendered.body).delayed(Duration::from_millis(800)),
        search_hit(81, &rendered.body),
        StubResponse::json(500, r#"{"error":"second create would duplicate"}"#),
    ]);
    let client = GitHubClient::new()
        .with_base_url(format!("http://{}", server.address()))
        .with_timeout(Duration::from_millis(150));

    let published = publish_work_item(&client, &destination, &publish_auth(), &draft)
        .expect("timeout after success must recover the fingerprint");
    assert_eq!(published.number, 81);
    assert_eq!(published.outcome, PublishOutcome::Existing);
    assert_eq!(published.fingerprint, rendered.fingerprint);

    let requests = server.requests();
    assert!(
        requests.len() >= 3,
        "expected search, timed-out create, recovery search; got {requests:?}"
    );
    assert!(request_is_search(&requests[0]));
    assert!(request_is_create(&requests[1]));
    assert!(request_is_search(&requests[2]));
    let creates = requests
        .iter()
        .filter(|request| request_is_create(request))
        .count();
    assert_eq!(
        creates, 1,
        "timeout recovery must not create a duplicate issue: {requests:?}"
    );
    assert!(
        !requests
            .iter()
            .any(|request| request.contains("api.github.com")),
        "tests must not write to live GitHub"
    );
}
