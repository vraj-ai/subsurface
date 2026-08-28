use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use thiserror::Error;

use crate::project::Project;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CandidateError {
    #[error("Project has no HEAD commit")]
    MissingHead,
    #[error("Git execution failed: {0}")]
    GitError(String),
    #[error("Candidate path escapes the disposable clone: {0}")]
    PathEscape(String),
    #[error("Failed to prepare candidate: {0}")]
    PreparationFailed(String),
    #[error("Active Project changed during candidate preparation")]
    ProjectMutated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFingerprint {
    pub head: String,
    pub porcelain: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateEdit {
    pub path: String,
    pub contents: String,
}

#[derive(Debug)]
pub struct PreparedCandidate {
    _clone: TempDir,
    pub clone_path: PathBuf,
    pub base_commit: String,
}

pub fn fingerprint_project(project: &Project) -> Result<GitFingerprint, CandidateError> {
    let root = &project.root_path;
    let head = git_stdout(root, &["rev-parse", "HEAD"])?;
    let porcelain = git_stdout(root, &["status", "--porcelain=v1", "--untracked-files=all"])?;
    Ok(GitFingerprint { head, porcelain })
}

pub fn prepare_candidate(
    project: &Project,
    edits: &[CandidateEdit],
) -> Result<PreparedCandidate, CandidateError> {
    prepare_candidate_with(project, |clone_root| apply_edits(clone_root, edits))
}

pub fn prepare_candidate_with<F>(
    project: &Project,
    work: F,
) -> Result<PreparedCandidate, CandidateError>
where
    F: FnOnce(&Path) -> Result<(), CandidateError>,
{
    let before = fingerprint_project(project)?;
    if before.head.is_empty() {
        return Err(CandidateError::MissingHead);
    }

    let parent = tempfile::tempdir()
        .map_err(|error| CandidateError::PreparationFailed(error.to_string()))?;
    let clone_path = parent.path().join("clone");
    clone_project(&project.root_path, &clone_path)?;

    let work_result = work(&clone_path);
    let after = fingerprint_project(project)?;
    if after != before {
        return Err(CandidateError::ProjectMutated);
    }
    work_result?;

    Ok(PreparedCandidate {
        _clone: parent,
        clone_path,
        base_commit: before.head,
    })
}

fn apply_edits(clone_root: &Path, edits: &[CandidateEdit]) -> Result<(), CandidateError> {
    for edit in edits {
        let dest = confined_path(clone_root, &edit.path)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| CandidateError::PreparationFailed(error.to_string()))?;
        }
        fs::write(&dest, &edit.contents)
            .map_err(|error| CandidateError::PreparationFailed(error.to_string()))?;
    }
    Ok(())
}

fn confined_path(root: &Path, relative: &str) -> Result<PathBuf, CandidateError> {
    let rel = Path::new(relative);
    if relative.is_empty()
        || rel.is_absolute()
        || rel.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(CandidateError::PathEscape(relative.to_string()));
    }
    Ok(root.join(rel))
}

fn clone_project(src: &Path, dst: &Path) -> Result<(), CandidateError> {
    let output = Command::new("git")
        .args([
            "clone",
            "--local",
            "--no-hardlinks",
            "--",
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
        ])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .map_err(|error| CandidateError::GitError(error.to_string()))?;
    if !output.status.success() {
        return Err(CandidateError::GitError(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

fn git_stdout(cwd: &Path, args: &[&str]) -> Result<String, CandidateError> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .map_err(|error| CandidateError::GitError(error.to_string()))?;
    if !output.status.success() {
        return Err(CandidateError::GitError(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
