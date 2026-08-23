use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// A deterministic Git fixture repository on disk for testing.
pub struct GitFixture {
    _temp_dir: Option<TempDir>,
    repo_path: PathBuf,
    commit_count: u32,
}

impl GitFixture {
    pub fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir for fixture");
        let repo_path = temp_dir.path().to_path_buf();
        let mut fixture = Self {
            _temp_dir: Some(temp_dir),
            repo_path,
            commit_count: 0,
        };
        fixture.init();
        fixture
    }

    pub fn new_at(path: &Path) -> Self {
        let mut fixture = Self {
            _temp_dir: None,
            repo_path: path.to_path_buf(),
            commit_count: 0,
        };
        fixture.init();
        fixture
    }

    pub fn path(&self) -> &Path {
        &self.repo_path
    }

    fn run_git(&self, args: &[&str], envs: &[(&str, &str)]) -> String {
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.repo_path);
        cmd.args(args);
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
        cmd.env("GIT_CONFIG_SYSTEM", "/dev/null");
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let output = cmd
            .output()
            .unwrap_or_else(|e| panic!("failed to execute git {:?}: {}", args, e));
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("git {:?} failed: {}", args, stderr);
        }
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn init(&mut self) {
        self.run_git(&["init", "-b", "main"], &[]);
        self.run_git(&["config", "user.name", "Subsurface Test"], &[]);
        self.run_git(&["config", "user.email", "test@subsurface.local"], &[]);
    }

    pub fn commit(&mut self, message: &str, files: &[(&str, &str)]) -> String {
        self.commit_count += 1;
        let ts = format!(
            "2026-01-01T{:02}:{:02}:00Z",
            self.commit_count / 60,
            self.commit_count % 60
        );

        for (rel_path, content) in files {
            let full_path = self.repo_path.join(rel_path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).expect("failed to create parent dir");
            }
            fs::write(&full_path, content).expect("failed to write file content");
            self.run_git(&["add", rel_path], &[]);
        }

        let envs = [
            ("GIT_AUTHOR_NAME", "Subsurface Test"),
            ("GIT_AUTHOR_EMAIL", "test@subsurface.local"),
            ("GIT_AUTHOR_DATE", ts.as_str()),
            ("GIT_COMMITTER_NAME", "Subsurface Test"),
            ("GIT_COMMITTER_EMAIL", "test@subsurface.local"),
            ("GIT_COMMITTER_DATE", ts.as_str()),
        ];

        self.run_git(&["commit", "-m", message], &envs);
        self.run_git(&["rev-parse", "HEAD"], &[])
    }

    pub fn rename_file(&mut self, old_path: &str, new_path: &str, message: &str) -> String {
        self.commit_count += 1;
        let ts = format!(
            "2026-01-01T{:02}:{:02}:00Z",
            self.commit_count / 60,
            self.commit_count % 60
        );

        let new_full = self.repo_path.join(new_path);
        if let Some(parent) = new_full.parent() {
            fs::create_dir_all(parent).expect("failed to create parent dir");
        }

        self.run_git(&["mv", old_path, new_path], &[]);

        let envs = [
            ("GIT_AUTHOR_NAME", "Subsurface Test"),
            ("GIT_AUTHOR_EMAIL", "test@subsurface.local"),
            ("GIT_AUTHOR_DATE", ts.as_str()),
            ("GIT_COMMITTER_NAME", "Subsurface Test"),
            ("GIT_COMMITTER_EMAIL", "test@subsurface.local"),
            ("GIT_COMMITTER_DATE", ts.as_str()),
        ];

        self.run_git(&["commit", "-m", message], &envs);
        self.run_git(&["rev-parse", "HEAD"], &[])
    }

    pub fn branch(&mut self, branch_name: &str) {
        self.run_git(&["branch", branch_name], &[]);
    }

    pub fn checkout(&mut self, branch_name: &str) {
        self.run_git(&["checkout", branch_name], &[]);
    }

    pub fn merge_branch(&mut self, branch_name: &str, message: &str) -> String {
        self.commit_count += 1;
        let ts = format!(
            "2026-01-01T{:02}:{:02}:00Z",
            self.commit_count / 60,
            self.commit_count % 60
        );

        let envs = [
            ("GIT_AUTHOR_NAME", "Subsurface Test"),
            ("GIT_AUTHOR_EMAIL", "test@subsurface.local"),
            ("GIT_AUTHOR_DATE", ts.as_str()),
            ("GIT_COMMITTER_NAME", "Subsurface Test"),
            ("GIT_COMMITTER_EMAIL", "test@subsurface.local"),
            ("GIT_COMMITTER_DATE", ts.as_str()),
        ];

        self.run_git(&["merge", "--no-ff", "-m", message, branch_name], &envs);
        self.run_git(&["rev-parse", "HEAD"], &[])
    }
}

impl Default for GitFixture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_fixture_creates_real_repo() {
        let mut fixture = GitFixture::new();
        let sha1 = fixture.commit("initial commit", &[("src/main.rs", "fn main() {}\n")]);
        assert_eq!(sha1.len(), 40);

        let sha2 = fixture.commit("add test file", &[("tests/main_test.rs", "// test\n")]);
        assert_ne!(sha1, sha2);

        assert!(fixture.path().join(".git").exists());
        assert!(fixture.path().join("src/main.rs").exists());
        assert!(fixture.path().join("tests/main_test.rs").exists());
    }

    #[test]
    fn test_git_fixture_rename_and_merge() {
        let mut fixture = GitFixture::new();
        fixture.commit("initial", &[("foo.txt", "hello")]);
        fixture.branch("feature");
        fixture.checkout("feature");
        fixture.commit("feature work", &[("bar.txt", "world")]);
        fixture.checkout("main");
        fixture.merge_branch("feature", "Merge branch feature");
        assert!(fixture.path().join("bar.txt").exists());

        fixture.rename_file("foo.txt", "baz.txt", "rename foo to baz");
        assert!(!fixture.path().join("foo.txt").exists());
        assert!(fixture.path().join("baz.txt").exists());
    }
}
