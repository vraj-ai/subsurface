use std::fs;
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::store::SqliteStore;
use subsurface_engine::workspace::discover_git_repositories;

#[test]
fn test_workspace_auto_discovery_and_pruning() {
    let temp_root = tempfile::tempdir().expect("tempdir");
    let root_path = temp_root.path();

    // 1. Create a valid repo A
    let repo_a = root_path.join("project_alpha");
    fs::create_dir_all(&repo_a).unwrap();
    let mut fix_a = GitFixture::new_at(&repo_a);
    fix_a.commit("init alpha", &[("README.md", "# Alpha\n")]);

    // 2. Create a valid nested repo B
    let repo_b = root_path.join("subfolder").join("project_beta");
    fs::create_dir_all(&repo_b).unwrap();
    let mut fix_b = GitFixture::new_at(&repo_b);
    fix_b.commit("init beta", &[("main.rs", "fn main() {}\n")]);

    // 3. Create a repo inside node_modules (must be pruned!)
    let ignored_dir = root_path.join("node_modules").join("dummy_pkg");
    fs::create_dir_all(&ignored_dir).unwrap();
    let mut fix_ignored = GitFixture::new_at(&ignored_dir);
    fix_ignored.commit("ignored", &[("index.js", "console.log('hi');\n")]);

    let discovered = discover_git_repositories(&[root_path.to_path_buf()], 3);

    assert_eq!(discovered.len(), 2, "Should discover exactly 2 repos (alpha & beta), pruning node_modules");
    let names: Vec<&str> = discovered.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"project_alpha"));
    assert!(names.contains(&"project_beta"));
    assert!(!names.contains(&"dummy_pkg"));
}

#[test]
fn test_workspace_persistence_and_pinning() {
    let store = SqliteStore::in_memory().expect("store");
    let fix = GitFixture::new();
    let path = fix.path();

    store.record_site_opened(path, "my_cool_project").expect("record opened");
    let sites = store.list_workspace_sites().expect("list");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].name, "my_cool_project");
    assert!(!sites[0].is_pinned);

    let is_pinned = store.toggle_pin_site(path).expect("toggle pin");
    assert!(is_pinned);

    let sites_after = store.list_workspace_sites().expect("list");
    assert!(sites_after[0].is_pinned);
}
