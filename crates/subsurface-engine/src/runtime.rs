use std::fmt;
use std::path::Path;
use std::time::Duration;

use crate::candidate::{prepare_candidate_with, CandidateError, PreparedCandidate};
use crate::command::{
    AllowedCommand, CommandAllowlist, CommandOutcome, IsolatedCommand, IsolatedRunner,
    IsolationError, ResourceBounds,
};
use crate::project::Project;
use crate::provider::OpenCodeBridge;

/// Optional OpenCode CLI used as a *runtime inside the product* to prepare a
/// candidate. Detection failure is a first-class unavailable state, not a hang.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCodeRuntime {
    bridge: Option<OpenCodeBridge>,
}

impl OpenCodeRuntime {
    pub fn none() -> Self {
        Self { bridge: None }
    }

    pub fn detect() -> Self {
        Self {
            bridge: OpenCodeBridge::detect(),
        }
    }

    pub fn from_bridge(bridge: OpenCodeBridge) -> Self {
        Self {
            bridge: Some(bridge),
        }
    }

    pub fn from_executable(executable: impl AsRef<Path>) -> Result<Self, CandidateError> {
        let bridge = OpenCodeBridge::from_executable(executable)
            .map_err(CandidateError::PreparationFailed)?;
        Ok(Self::from_bridge(bridge))
    }

    pub fn is_available(&self) -> bool {
        self.bridge.is_some()
    }
}

/// Why preparation cannot start, plus the next verb the user can take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnavailableState {
    pub reason: String,
    pub next_verb: String,
}

impl UnavailableState {
    fn no_runtime() -> Self {
        Self {
            reason: "OpenCode runtime is unavailable.".into(),
            next_verb:
                "Install OpenCode or connect a runtime in Connections to prepare a candidate."
                    .into(),
        }
    }

    pub fn message(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for UnavailableState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.reason, self.next_verb)
    }
}

#[derive(Debug)]
pub enum PrepareOutcome {
    Prepared(PreparedCandidate),
    Unavailable(UnavailableState),
}

/// Prepare a candidate through the optional OpenCode runtime.
///
/// When no runtime is configured, returns an actionable unavailable state
/// without cloning, hanging, or touching the active Project. When a runtime is
/// present, preparation runs inside a disposable clone through the isolated
/// command runner.
pub fn prepare_via_runtime(
    project: &Project,
    runtime: &OpenCodeRuntime,
    prompt: &str,
) -> Result<PrepareOutcome, CandidateError> {
    let Some(bridge) = runtime.bridge.as_ref() else {
        return Ok(PrepareOutcome::Unavailable(UnavailableState::no_runtime()));
    };

    let executable = bridge.executable().to_path_buf();
    let prompt = prompt.to_owned();
    let runner_project = project.clone();

    let prepared = prepare_candidate_with(project, move |clone_root| {
        let program = executable.to_string_lossy().into_owned();
        let basename = executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("opencode")
            .to_owned();
        let allowlist = CommandAllowlist::new(vec![AllowedCommand::new(basename)
            .with_arg_prefix(["run"])
            .with_network(true)]);
        let bounds = ResourceBounds {
            timeout: Duration::from_secs(30),
            max_output_bytes: 1_048_576,
            max_cpu: Duration::from_secs(30),
            max_concurrency: 1,
        };
        let runner = IsolatedRunner::new(runner_project, clone_root, allowlist, bounds);
        let receipt = runner
            .run(&IsolatedCommand::new(
                program,
                [String::from("run"), prompt],
            ))
            .map_err(isolation_to_candidate)?;
        match receipt.outcome {
            CommandOutcome::Succeeded => Ok(()),
            other => Err(CandidateError::PreparationFailed(format!(
                "OpenCode runtime {other:?}: {}",
                receipt.stderr.trim()
            ))),
        }
    })?;

    Ok(PrepareOutcome::Prepared(prepared))
}

fn isolation_to_candidate(error: IsolationError) -> CandidateError {
    match error {
        IsolationError::ProjectMutated => CandidateError::ProjectMutated,
        IsolationError::PathEscape(path) => CandidateError::PathEscape(path),
        IsolationError::GitError(message) => CandidateError::GitError(message),
        other => CandidateError::PreparationFailed(other.to_string()),
    }
}
