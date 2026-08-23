use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::site::Site;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveredSiteInfo {
    pub name: String,
    pub path: PathBuf,
    pub head_commit: Option<String>,
    pub total_commits_estimate: usize,
    pub is_shallow: bool,
    pub has_remote: bool,
    pub is_pinned: bool,
    pub field_notes_count: usize,
    pub dead_workarounds_count: usize,
}

/// Discover git repositories under a set of root directories up to `max_depth`.
/// Prunes heavy build/dependency directories to guarantee execution in under 200ms.
pub fn discover_git_repositories(roots: &[PathBuf], max_depth: usize) -> Vec<DiscoveredSiteInfo> {
    let mut discovered = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for root in roots {
        if !root.exists() || !root.is_dir() {
            continue;
        }

        // Check if root itself is a git repo
        if root.join(".git").exists() {
            let path_buf = root.to_path_buf();
            if seen_paths.insert(path_buf.clone()) {
                if let Ok(site) = Site::open(&path_buf) {
                    discovered.push(DiscoveredSiteInfo {
                        name: root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "repo".to_string()),
                        path: site.root_path,
                        head_commit: site.head_commit,
                        total_commits_estimate: site.total_commits_estimate,
                        is_shallow: site.is_shallow,
                        has_remote: site.has_remote,
                        is_pinned: false,
                        field_notes_count: 0,
                        dead_workarounds_count: 0,
                    });
                }
            }
            continue;
        }

        scan_directory_recursive(root, 1, max_depth, &mut discovered, &mut seen_paths);
    }

    discovered
}

fn scan_directory_recursive(
    dir: &Path,
    current_depth: usize,
    max_depth: usize,
    results: &mut Vec<DiscoveredSiteInfo>,
    seen: &mut std::collections::HashSet<PathBuf>,
) {
    if current_depth > max_depth {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

        // Pruning heuristic
        if is_ignored_directory(&file_name) {
            continue;
        }

        if path.join(".git").exists() {
            if seen.insert(path.clone()) {
                if let Ok(site) = Site::open(&path) {
                    results.push(DiscoveredSiteInfo {
                        name: file_name,
                        path: site.root_path,
                        head_commit: site.head_commit,
                        total_commits_estimate: site.total_commits_estimate,
                        is_shallow: site.is_shallow,
                        has_remote: site.has_remote,
                        is_pinned: false,
                        field_notes_count: 0,
                        dead_workarounds_count: 0,
                    });
                }
            }
        } else {
            scan_directory_recursive(&path, current_depth + 1, max_depth, results, seen);
        }
    }
}

fn is_ignored_directory(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | "target"
            | ".cargo"
            | ".git"
            | "vendor"
            | "dist"
            | "build"
            | ".cache"
            | ".next"
            | "Library"
            | "System"
            | "Applications"
            | ".Trash"
    ) || name.starts_with('.')
}

/// Default standard search directories on macOS/Linux
pub fn get_default_scan_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for folder in ["Work", "Projects", "dev", "code", "src", "Documents"] {
            let p = home.join(folder);
            if p.exists() && p.is_dir() {
                roots.push(p);
            }
        }
    }
    roots
}
