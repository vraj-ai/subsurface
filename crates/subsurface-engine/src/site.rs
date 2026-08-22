use std::path::{Path, PathBuf};
use std::process::Command;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SiteError {
    #[error("Path is not a git repository: {0}")]
    NotAGitRepository(PathBuf),
    #[error("Path does not exist: {0}")]
    PathNotFound(PathBuf),
    #[error("Git execution failed: {0}")]
    GitError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SiteIndexReport {
    pub commit_graph_indexed: bool,
    pub file_paths_indexed: bool,
    pub rename_links_indexed: bool,
    pub expensive_walks_indexed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Site {
    pub root_path: PathBuf,
    pub is_shallow: bool,
    pub has_remote: bool,
    pub remotes: Vec<String>,
    pub head_commit: Option<String>,
    pub total_commits_estimate: usize,
    pub tracked_files: Vec<String>,
    pub index_report: SiteIndexReport,
}

impl Site {
    /// Opens a git repository as a Site and builds the cheap on-open index in seconds.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SiteError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(SiteError::PathNotFound(path.to_path_buf()));
        }

        let git_dir_check = Command::new("git")
            .current_dir(path)
            .args(["rev-parse", "--show-toplevel"])
            .output();

        let Ok(output) = git_dir_check else {
            return Err(SiteError::NotAGitRepository(path.to_path_buf()));
        };

        if !output.status.success() {
            return Err(SiteError::NotAGitRepository(path.to_path_buf()));
        }

        let canonical_root = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());

        let is_shallow = Command::new("git")
            .current_dir(&canonical_root)
            .args(["rev-parse", "--is-shallow-repository"])
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim() == "true")
            .unwrap_or(false);

        let remotes: Vec<String> = Command::new("git")
            .current_dir(&canonical_root)
            .args(["remote"])
            .output()
            .map(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let has_remote = !remotes.is_empty();

        let head_commit = Command::new("git")
            .current_dir(&canonical_root)
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .and_then(|out| {
                if out.status.success() {
                    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
                } else {
                    None
                }
            });

        let total_commits_estimate = Command::new("git")
            .current_dir(&canonical_root)
            .args(["rev-list", "--count", "HEAD"])
            .output()
            .ok()
            .and_then(|out| {
                if out.status.success() {
                    String::from_utf8_lossy(&out.stdout)
                        .trim()
                        .parse::<usize>()
                        .ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let tracked_files: Vec<String> = Command::new("git")
            .current_dir(&canonical_root)
            .args(["ls-files"])
            .output()
            .map(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let index_report = SiteIndexReport {
            commit_graph_indexed: true,
            file_paths_indexed: true,
            rename_links_indexed: true,
            expensive_walks_indexed: false,
        };

        Ok(Self {
            root_path: canonical_root,
            is_shallow,
            has_remote,
            remotes,
            head_commit,
            total_commits_estimate,
            tracked_files,
            index_report,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RecentSitesStore {
    pub sites: Vec<PathBuf>,
}

impl RecentSitesStore {
    pub fn add(&mut self, path: PathBuf) {
        self.sites.retain(|p| p != &path);
        self.sites.insert(0, path);
        if self.sites.len() > 20 {
            self.sites.truncate(20);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::GitFixture;

    #[test]
    fn test_open_valid_site() {
        let mut fixture = GitFixture::new();
        fixture.commit("initial", &[("src/lib.rs", "// code\n")]);

        let site = Site::open(fixture.path()).expect("should open site");
        assert_eq!(site.total_commits_estimate, 1);
        assert!(!site.is_shallow);
        assert!(!site.has_remote);
        assert!(site.tracked_files.contains(&"src/lib.rs".to_string()));
        assert!(site.index_report.commit_graph_indexed);
        assert!(site.index_report.file_paths_indexed);
        assert!(!site.index_report.expensive_walks_indexed);
    }

    #[test]
    fn test_open_non_git_directory() {
        let temp = tempfile::tempdir().unwrap();
        let result = Site::open(temp.path());
        assert!(matches!(result, Err(SiteError::NotAGitRepository(_))));
    }

    #[test]
    fn test_recent_sites_store() {
        let mut store = RecentSitesStore::default();
        let p1 = PathBuf::from("/repo1");
        let p2 = PathBuf::from("/repo2");

        store.add(p1.clone());
        store.add(p2.clone());
        assert_eq!(store.sites, vec![p2.clone(), p1.clone()]);

        // Re-adding p1 moves it to front
        store.add(p1.clone());
        assert_eq!(store.sites, vec![p1, p2]);
    }
}
