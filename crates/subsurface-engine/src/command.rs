use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::candidate::{fingerprint_project, CandidateError, GitFingerprint};
use crate::project::Project;

const SAFE_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "TMPDIR",
    "TMP",
    "TEMP",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "TERM",
    "TZ",
    "SHELL",
];

const SECRET_MARKERS: &[&str] = &[
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "CREDENTIAL",
    "API_KEY",
    "PRIVATE_KEY",
    "ACCESS_KEY",
    "AUTH",
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IsolationError {
    #[error("command is not on the Project allowlist: {program}")]
    NotAllowlisted { program: String, args: Vec<String> },
    #[error("command path escapes the disposable clone: {0}")]
    PathEscape(String),
    #[error("too many candidate commands already running")]
    ConcurrencyLimit,
    #[error("failed to start isolated command: {0}")]
    Spawn(String),
    #[error("Active Project changed during isolated command")]
    ProjectMutated,
    #[error("Git execution failed: {0}")]
    GitError(String),
}

impl From<CandidateError> for IsolationError {
    fn from(error: CandidateError) -> Self {
        match error {
            CandidateError::ProjectMutated => IsolationError::ProjectMutated,
            CandidateError::GitError(message) | CandidateError::PreparationFailed(message) => {
                IsolationError::GitError(message)
            }
            CandidateError::PathEscape(path) => IsolationError::PathEscape(path),
            other => IsolationError::GitError(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowedCommand {
    pub program: String,
    pub arg_prefix: Vec<String>,
    pub network: bool,
}

impl AllowedCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            arg_prefix: Vec::new(),
            network: false,
        }
    }

    pub fn with_arg_prefix<I, S>(mut self, prefix: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg_prefix = prefix.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_network(mut self, network: bool) -> Self {
        self.network = network;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandAllowlist {
    entries: Vec<AllowedCommand>,
}

impl CommandAllowlist {
    pub fn new(entries: Vec<AllowedCommand>) -> Self {
        Self { entries }
    }

    pub fn preview(&self) -> &[AllowedCommand] {
        &self.entries
    }

    fn matching(&self, program: &str, args: &[String]) -> Option<&AllowedCommand> {
        let base = program_basename(program);
        self.entries.iter().find(|entry| {
            program_basename(&entry.program) == base
                && args.starts_with(entry.arg_prefix.as_slice())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceBounds {
    pub timeout: Duration,
    pub max_output_bytes: usize,
    pub max_cpu: Duration,
    pub max_concurrency: usize,
}

impl Default for ResourceBounds {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_output_bytes: 1_048_576,
            max_cpu: Duration::from_secs(30),
            max_concurrency: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolatedCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl IsolatedCommand {
    pub fn new(
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOutcome {
    Succeeded,
    Failed,
    TimedOut,
    OutputLimited,
    CpuLimited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandReceipt {
    pub program: String,
    pub args: Vec<String>,
    pub outcome: CommandOutcome,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub fingerprint_before: GitFingerprint,
    pub fingerprint_after: GitFingerprint,
}

#[derive(Clone)]
pub struct IsolatedRunner {
    project: Project,
    clone_root: PathBuf,
    allowlist: CommandAllowlist,
    bounds: ResourceBounds,
    running: Arc<AtomicUsize>,
}

impl IsolatedRunner {
    pub fn new(
        project: Project,
        clone_root: impl Into<PathBuf>,
        allowlist: CommandAllowlist,
        bounds: ResourceBounds,
    ) -> Self {
        Self {
            project,
            clone_root: clone_root.into(),
            allowlist,
            bounds,
            running: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn run(&self, command: &IsolatedCommand) -> Result<CommandReceipt, IsolationError> {
        let allowed = self
            .allowlist
            .matching(&command.program, &command.args)
            .cloned()
            .ok_or_else(|| IsolationError::NotAllowlisted {
                program: command.program.clone(),
                args: command.args.clone(),
            })?;

        if command
            .program
            .as_str()
            .split(['/', '\\'])
            .any(|part| part == "..")
        {
            return Err(IsolationError::PathEscape(command.program.clone()));
        }

        let before = fingerprint_project(&self.project)?;
        let _slot = self.acquire_slot()?;
        let run = self.spawn_and_reap(&allowed, command)?;
        let after = fingerprint_project(&self.project)?;
        if after != before {
            return Err(IsolationError::ProjectMutated);
        }

        Ok(CommandReceipt {
            program: command.program.clone(),
            args: command.args.clone(),
            outcome: run.outcome,
            stdout: run.stdout,
            stderr: run.stderr,
            exit_code: run.exit_code,
            fingerprint_before: before,
            fingerprint_after: after,
        })
    }

    fn acquire_slot(&self) -> Result<ConcurrencySlot, IsolationError> {
        loop {
            let current = self.running.load(Ordering::SeqCst);
            if current >= self.bounds.max_concurrency.max(1) {
                return Err(IsolationError::ConcurrencyLimit);
            }
            if self
                .running
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Ok(ConcurrencySlot {
                    running: Arc::clone(&self.running),
                });
            }
        }
    }

    fn spawn_and_reap(
        &self,
        allowed: &AllowedCommand,
        command: &IsolatedCommand,
    ) -> Result<RawRun, IsolationError> {
        let path_env = std::env::var("PATH").unwrap_or_default();
        let resolved = resolve_program(&command.program, &path_env, &self.clone_root)?;
        let mut process = sandbox_command(allowed.network, &self.project.root_path, &resolved);
        process
            .args(&command.args)
            .current_dir(&self.clone_root)
            .env_clear()
            .envs(scrubbed_env())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        apply_cpu_and_pgroup(&mut process, self.bounds.max_cpu);

        let mut child = process
            .spawn()
            .map_err(|error| IsolationError::Spawn(error.to_string()))?;

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| IsolationError::Spawn("missing stdout pipe".into()))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| IsolationError::Spawn("missing stderr pipe".into()))?;

        let remaining = Arc::new(AtomicUsize::new(self.bounds.max_output_bytes));
        let (trunc_tx, trunc_rx) = mpsc::channel();
        let stdout_remaining = Arc::clone(&remaining);
        let stderr_remaining = Arc::clone(&remaining);
        let stdout_tx = trunc_tx.clone();
        let stderr_tx = trunc_tx;

        let stdout_handle =
            thread::spawn(move || read_capped(&mut stdout, stdout_remaining, stdout_tx));
        let stderr_handle =
            thread::spawn(move || read_capped(&mut stderr, stderr_remaining, stderr_tx));

        let started = Instant::now();
        let mut timed_out = false;
        let mut output_limited = false;
        let wait_result = loop {
            if trunc_rx.try_recv().is_ok() {
                output_limited = true;
                kill_process_group(&mut child);
                break child.wait();
            }
            if started.elapsed() >= self.bounds.timeout {
                timed_out = true;
                kill_process_group(&mut child);
                break child.wait();
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => thread::sleep(Duration::from_millis(5)),
                Err(error) => break Err(error),
            }
        };

        let status = wait_result.map_err(|error| IsolationError::Spawn(error.to_string()))?;
        let stdout = stdout_handle.join().unwrap_or_default();
        let stderr = stderr_handle.join().unwrap_or_default();

        let signaled_cpu = cpu_signal(&status);
        let outcome = if timed_out {
            CommandOutcome::TimedOut
        } else if output_limited {
            CommandOutcome::OutputLimited
        } else if signaled_cpu {
            CommandOutcome::CpuLimited
        } else if status.success() {
            CommandOutcome::Succeeded
        } else {
            CommandOutcome::Failed
        };

        Ok(RawRun {
            outcome,
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            exit_code: status.code(),
        })
    }
}

struct ConcurrencySlot {
    running: Arc<AtomicUsize>,
}

impl Drop for ConcurrencySlot {
    fn drop(&mut self) {
        self.running.fetch_sub(1, Ordering::SeqCst);
    }
}

struct RawRun {
    outcome: CommandOutcome,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
}

fn program_basename(program: &str) -> &str {
    Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
}

fn resolve_program(
    program: &str,
    path_env: &str,
    clone_root: &Path,
) -> Result<PathBuf, IsolationError> {
    let path = Path::new(program);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(IsolationError::PathEscape(program.to_string()));
    }
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    if program.contains('/') {
        return Ok(clone_root.join(path));
    }
    for dir in std::env::split_paths(path_env) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(IsolationError::Spawn(format!(
        "allowlisted program not found on PATH: {program}"
    )))
}

fn looks_secret(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    SECRET_MARKERS.iter().any(|marker| upper.contains(marker))
        || upper.starts_with("AWS_")
        || upper.starts_with("GITHUB_")
        || upper.starts_with("SSH_")
        || upper.starts_with("OPENAI_")
        || upper.starts_with("XAI_")
}

fn scrubbed_env() -> Vec<(String, String)> {
    std::env::vars()
        .filter(|(key, _)| {
            SAFE_ENV.iter().any(|keep| key.eq_ignore_ascii_case(keep)) && !looks_secret(key)
        })
        .collect()
}

fn sandbox_command(network: bool, project_root: &Path, program: &Path) -> Command {
    #[cfg(target_os = "macos")]
    {
        let profile = sandbox_profile(network, project_root);
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command.arg("-p").arg(profile).arg(program);
        command
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = network;
        let _ = project_root;
        Command::new(program)
    }
}

#[cfg(target_os = "macos")]
fn sandbox_profile(network: bool, project_root: &Path) -> String {
    let mut paths = vec![escape_sandbox_path(project_root)];
    if let Ok(canonical) = project_root.canonicalize() {
        let escaped = escape_sandbox_path(&canonical);
        if !paths.contains(&escaped) {
            paths.push(escaped);
        }
    }
    let writes = paths
        .iter()
        .map(|path| format!("(subpath \"{path}\")"))
        .collect::<Vec<_>>()
        .join(" ");
    let network_rule = if network {
        String::new()
    } else {
        "(deny network*)\n(deny network-outbound)\n(deny network-inbound)\n(deny network-bind)\n"
            .to_string()
    };
    format!("(version 1)\n(allow default)\n(deny file-write* {writes})\n{network_rule}")
}

#[cfg(target_os = "macos")]
fn escape_sandbox_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn apply_cpu_and_pgroup(command: &mut Command, max_cpu: Duration) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let cpu_seconds = max_cpu.as_secs().max(1);
        unsafe {
            command.pre_exec(move || unix_limits::apply(cpu_seconds));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = command;
        let _ = max_cpu;
    }
}

fn kill_process_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    unix_limits::kill_group(child.id());
    let _ = child.kill();
}

fn cpu_signal(status: &std::process::ExitStatus) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        matches!(status.signal(), Some(unix_limits::SIGXCPU))
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        false
    }
}

fn read_capped<R: Read>(
    reader: &mut R,
    remaining: Arc<AtomicUsize>,
    truncated: mpsc::Sender<()>,
) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let allowed = remaining.load(Ordering::SeqCst);
                if allowed == 0 {
                    let _ = truncated.send(());
                    break;
                }
                let take = n.min(allowed);
                buf.extend_from_slice(&chunk[..take]);
                let leftover = remaining.fetch_sub(take, Ordering::SeqCst);
                if leftover <= take || take < n {
                    remaining.store(0, Ordering::SeqCst);
                    let _ = truncated.send(());
                    break;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    buf
}

#[cfg(unix)]
mod unix_limits {
    use std::io;

    pub const SIGXCPU: i32 = 24;
    const SIGKILL: i32 = 9;
    const RLIMIT_CPU: i32 = 0;

    #[repr(C)]
    struct Rlimit {
        rlim_cur: u64,
        rlim_max: u64,
    }

    extern "C" {
        fn setrlimit(resource: i32, rlim: *const Rlimit) -> i32;
        fn setpgid(pid: i32, pgid: i32) -> i32;
        fn killpg(pgrp: i32, sig: i32) -> i32;
    }

    pub fn apply(cpu_seconds: u64) -> io::Result<()> {
        let limit = Rlimit {
            rlim_cur: cpu_seconds,
            rlim_max: cpu_seconds,
        };
        unsafe {
            let _ = setpgid(0, 0);
            let _ = setrlimit(RLIMIT_CPU, &limit);
        }
        Ok(())
    }

    pub fn kill_group(pid: u32) {
        unsafe {
            let _ = killpg(pid as i32, SIGKILL);
        }
    }
}
