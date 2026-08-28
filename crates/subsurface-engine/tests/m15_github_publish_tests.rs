mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use subsurface_engine::fixture::GitFixture;
use subsurface_engine::github::{
    github_oauth_client, infer_github_destination, resolve_github_auth,
    resolve_github_publish_target, GhCli, GhPermission, GitHubAuthMethod, GitHubAuthProbes,
    GitHubError, WorkItemDestination,
};
use subsurface_engine::oauth::OAuthClient;
use subsurface_engine::project::Project;
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
