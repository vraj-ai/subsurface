use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::oauth::{DevicePollResult, OAuthClient, OAuthError};
use crate::project::Project;

/// GitHub is the only Work Item destination in v1.1.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemDestination {
    GitHub,
}

impl WorkItemDestination {
    pub const ONLY: Self = Self::GitHub;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GitHubError {
    #[error("Project has no git remotes")]
    NoRemote,
    #[error("No GitHub remote found; GitHub is the only Work Item destination")]
    NotGitHub,
    #[error("Git execution failed: {0}")]
    GitError(String),
    #[error("GitHub authentication method is unavailable: {0}")]
    Unavailable(String),
    #[error("GitHub authentication failed: {0}")]
    AuthFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubDestination {
    pub owner: String,
    pub repo: String,
    pub remote_name: String,
    pub remote_url: String,
}

impl GitHubDestination {
    pub fn kind(&self) -> WorkItemDestination {
        WorkItemDestination::GitHub
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitHubAuthMethod {
    GhCli,
    Browser,
    Device,
    Token,
}

impl GitHubAuthMethod {
    /// Canonical auth fallback order after destination inference.
    pub const FALLBACK_ORDER: [Self; 4] = [Self::GhCli, Self::Browser, Self::Device, Self::Token];
}

#[derive(Clone, PartialEq, Eq)]
pub struct GitHubAuth {
    pub method: GitHubAuthMethod,
    pub token: String,
    pub attempted: Vec<GitHubAuthMethod>,
}

impl fmt::Debug for GitHubAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitHubAuth")
            .field("method", &self.method)
            .field("token", &"[redacted]")
            .field("attempted", &self.attempted)
            .finish()
    }
}

/// Permission to invoke the GitHub CLI. Denied until the user grants it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GhPermission {
    granted: bool,
}

impl GhPermission {
    pub fn granted() -> Self {
        Self { granted: true }
    }

    pub fn denied() -> Self {
        Self { granted: false }
    }

    pub fn is_granted(self) -> bool {
        self.granted
    }
}

/// Local `gh` binary used only after [`GhPermission`] is granted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhCli {
    executable: PathBuf,
}

impl GhCli {
    pub fn detect() -> Option<Self> {
        Self::from_executable("gh").ok()
    }

    pub fn from_executable(executable: impl AsRef<Path>) -> Result<Self, GitHubError> {
        let executable = executable.as_ref().to_path_buf();
        if executable.as_os_str().is_empty() {
            return Err(GitHubError::Unavailable("GitHub CLI path is empty".into()));
        }
        Ok(Self { executable })
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn token(&self) -> Result<String, GitHubError> {
        let output = Command::new(&self.executable)
            .args(["auth", "token"])
            .env("GH_PROMPT_DISABLED", "1")
            .output()
            .map_err(|error| GitHubError::Unavailable(error.to_string()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(GitHubError::Unavailable(if stderr.is_empty() {
                format!("gh auth token exited {}", output.status)
            } else {
                stderr
            }));
        }
        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if token.is_empty() {
            return Err(GitHubError::Unavailable(
                "gh auth token returned no token".into(),
            ));
        }
        Ok(token)
    }
}

/// Injectable auth sources. Tests point OAuth at a local fake and never write
/// GitHub issues.
pub struct GitHubAuthProbes {
    gh: Option<GhCli>,
    oauth: Option<OAuthClient>,
    redirect_uri: String,
    scopes: Vec<String>,
    browser_code: Option<String>,
    pasted_token: Option<String>,
}

impl Default for GitHubAuthProbes {
    fn default() -> Self {
        Self::new()
    }
}

impl GitHubAuthProbes {
    pub fn new() -> Self {
        Self {
            gh: None,
            oauth: None,
            redirect_uri: "http://127.0.0.1/callback".into(),
            scopes: vec!["repo".into()],
            browser_code: None,
            pasted_token: None,
        }
    }

    pub fn with_gh(mut self, gh: GhCli) -> Self {
        self.gh = Some(gh);
        self
    }

    pub fn with_oauth(mut self, oauth: OAuthClient, redirect_uri: impl Into<String>) -> Self {
        self.oauth = Some(oauth);
        self.redirect_uri = redirect_uri.into();
        self
    }

    pub fn with_scopes<I, S>(mut self, scopes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.scopes = scopes.into_iter().map(Into::into).collect();
        self
    }

    /// Completes the browser flow non-interactively with an authorization code.
    /// Production UI supplies the callback; tests inject a fake code.
    pub fn with_browser_code(mut self, code: impl Into<String>) -> Self {
        self.browser_code = Some(code.into());
        self
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.pasted_token = Some(token.into());
        self
    }

    fn try_gh(&self) -> Result<String, GitHubError> {
        let gh = self
            .gh
            .as_ref()
            .ok_or_else(|| GitHubError::Unavailable("GitHub CLI is not configured".into()))?;
        gh.token()
    }

    fn try_browser(&self) -> Result<String, GitHubError> {
        let oauth = self
            .oauth
            .as_ref()
            .ok_or_else(|| GitHubError::Unavailable("browser OAuth is not configured".into()))?;
        let code = self.browser_code.as_ref().ok_or_else(|| {
            GitHubError::Unavailable("browser OAuth callback is not available".into())
        })?;
        let scopes: Vec<&str> = self.scopes.iter().map(String::as_str).collect();
        let mut authorization = oauth
            .start_authorization(&self.redirect_uri, &scopes)
            .map_err(oauth_error)?;
        let callback = format!(
            "{}{}code={}&state={}",
            self.redirect_uri,
            if self.redirect_uri.contains('?') {
                "&"
            } else {
                "?"
            },
            code,
            authorization.state
        );
        oauth
            .exchange_callback(&mut authorization, &callback)
            .map(|tokens| tokens.access_token)
            .map_err(oauth_error)
    }

    fn try_device(&self) -> Result<String, GitHubError> {
        let oauth = self
            .oauth
            .as_ref()
            .ok_or_else(|| GitHubError::Unavailable("device OAuth is not configured".into()))?;
        let scopes: Vec<&str> = self.scopes.iter().map(String::as_str).collect();
        let mut authorization = oauth
            .start_device_authorization(&scopes)
            .map_err(oauth_error)?;
        for _ in 0..3 {
            match oauth
                .poll_device_tokens(&mut authorization)
                .map_err(oauth_error)?
            {
                DevicePollResult::Complete(tokens) => return Ok(tokens.access_token),
                DevicePollResult::Pending { .. } => {}
            }
        }
        Err(GitHubError::Unavailable(
            "device authorization did not complete".into(),
        ))
    }

    fn try_token(&self) -> Result<String, GitHubError> {
        self.pasted_token
            .as_ref()
            .map(|token| token.trim())
            .filter(|token| !token.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| GitHubError::Unavailable("no pasted GitHub token".into()))
    }
}

/// Infer the GitHub owner/repo from Project remotes. Prefers `origin` when it
/// points at github.com; otherwise the first GitHub remote. Non-GitHub forges
/// are never a Work Item destination.
pub fn infer_github_destination(project: &Project) -> Result<GitHubDestination, GitHubError> {
    let remotes = list_fetch_remotes(&project.root_path)?;
    if remotes.is_empty() {
        return Err(GitHubError::NoRemote);
    }

    if let Some(origin) = remotes.iter().find(|remote| remote.name == "origin") {
        if let Some(destination) = destination_from_remote(origin) {
            return Ok(destination);
        }
    }

    remotes
        .iter()
        .find_map(destination_from_remote)
        .ok_or(GitHubError::NotGitHub)
}

/// Resolve GitHub auth. `gh` runs only after permission; then browser, device,
/// and pasted token, in that order.
pub fn resolve_github_auth(
    permission: GhPermission,
    probes: &GitHubAuthProbes,
) -> Result<GitHubAuth, GitHubError> {
    let mut attempted = Vec::new();
    let mut last_error =
        GitHubError::AuthFailed("no GitHub authentication method succeeded".into());

    for method in GitHubAuthMethod::FALLBACK_ORDER {
        if method == GitHubAuthMethod::GhCli && !permission.is_granted() {
            continue;
        }
        attempted.push(method);
        let result = match method {
            GitHubAuthMethod::GhCli => probes.try_gh(),
            GitHubAuthMethod::Browser => probes.try_browser(),
            GitHubAuthMethod::Device => probes.try_device(),
            GitHubAuthMethod::Token => probes.try_token(),
        };
        match result {
            Ok(token) => {
                return Ok(GitHubAuth {
                    method,
                    token,
                    attempted,
                });
            }
            Err(error) => last_error = error,
        }
    }

    Err(match last_error {
        GitHubError::AuthFailed(message) => GitHubError::AuthFailed(message),
        other => GitHubError::AuthFailed(other.to_string()),
    })
}

pub fn resolve_github_publish_target(
    project: &Project,
    permission: GhPermission,
    probes: &GitHubAuthProbes,
) -> Result<(GitHubDestination, GitHubAuth), GitHubError> {
    let destination = infer_github_destination(project)?;
    let auth = resolve_github_auth(permission, probes)?;
    Ok((destination, auth))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteUrl {
    name: String,
    url: String,
}

fn destination_from_remote(remote: &RemoteUrl) -> Option<GitHubDestination> {
    let (owner, repo) = parse_github_owner_repo(&remote.url)?;
    Some(GitHubDestination {
        owner,
        repo,
        remote_name: remote.name.clone(),
        remote_url: remote.url.clone(),
    })
}

pub fn parse_github_owner_repo(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }

    if let Some((host_part, path_part)) = scp_like_parts(url) {
        if !is_github_host(host_part) {
            return None;
        }
        return split_owner_repo(path_part);
    }

    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    if !is_github_host(host) {
        return None;
    }
    split_owner_repo(parsed.path())
}

fn scp_like_parts(url: &str) -> Option<(&str, &str)> {
    if url.contains("://") {
        return None;
    }
    let (host_part, path_part) = url.split_once(':')?;
    if path_part.starts_with("//") {
        return None;
    }
    let host = host_part.rsplit('@').next()?;
    Some((host, path_part))
}

fn is_github_host(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    host == "github.com" || host == "www.github.com"
}

fn split_owner_repo(path: &str) -> Option<(String, String)> {
    let path = path.trim().trim_start_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let path = path.trim_end_matches('/');
    let mut parts = path.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty()
        || repo.is_empty()
        || parts.next().is_some()
        || owner == "."
        || owner == ".."
        || repo == "."
        || repo == ".."
    {
        return None;
    }
    Some((owner.to_owned(), repo.to_owned()))
}

fn list_fetch_remotes(root: &Path) -> Result<Vec<RemoteUrl>, GitHubError> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["remote", "-v"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .map_err(|error| GitHubError::GitError(error.to_string()))?;
    if !output.status.success() {
        return Err(GitHubError::GitError(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut remotes = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if !line.ends_with("(fetch)") {
            continue;
        }
        let without_kind = line.trim_end_matches("(fetch)").trim();
        let Some((name, url)) = without_kind.split_once(|ch: char| ch.is_whitespace()) else {
            continue;
        };
        let name = name.trim();
        let url = url.trim();
        if name.is_empty() || url.is_empty() {
            continue;
        }
        if remotes.iter().any(|remote: &RemoteUrl| remote.name == name) {
            continue;
        }
        remotes.push(RemoteUrl {
            name: name.to_owned(),
            url: url.to_owned(),
        });
    }
    Ok(remotes)
}

fn oauth_error(error: OAuthError) -> GitHubError {
    match error {
        OAuthError::DeviceUnsupported => {
            GitHubError::Unavailable("device authorization is not exposed".into())
        }
        OAuthError::Cancelled => GitHubError::Unavailable("OAuth flow was cancelled".into()),
        other => GitHubError::AuthFailed(other.to_string()),
    }
}

pub fn github_oauth_client(
    client_id: impl Into<String>,
    auth_url: impl Into<String>,
    token_url: impl Into<String>,
    device_authorization_url: impl Into<String>,
) -> OAuthClient {
    OAuthClient::new(client_id, auth_url, token_url)
        .with_device_authorization_url(device_authorization_url)
        .with_timeout(Duration::from_secs(45))
}
