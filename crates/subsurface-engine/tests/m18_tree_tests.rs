use subsurface_engine::fixture::GitFixture;
use subsurface_engine::project::Project;
use subsurface_engine::store::SqliteStore;
use subsurface_engine::tree::{ProjectTree, TreeEntryKind, TreeKey};

fn sample_project() -> (GitFixture, Project) {
    let mut fixture = GitFixture::new();
    fixture.commit(
        "tree layout",
        &[
            ("README.md", "# sample\n"),
            ("docs/guide.md", "guide\n"),
            ("src/lib.rs", "// lib\n"),
            ("src/main.rs", "fn main() {}\n"),
            ("src/nested/mod.rs", "mod nested;\n"),
            ("tests/tree.rs", "// tests\n"),
        ],
    );
    let project = Project::open(fixture.path()).expect("open project");
    (fixture, project)
}

fn visible_paths(tree: &ProjectTree) -> Vec<String> {
    tree.visible_rows()
        .into_iter()
        .map(|row| row.path)
        .collect()
}

#[test]
fn builds_hierarchical_tree_with_directories_before_files() {
    let (_fixture, project) = sample_project();
    std::fs::write(project.root_path.join("untracked.txt"), "ignore me\n")
        .expect("write untracked file");

    let tree = ProjectTree::from_project(&project);
    let roots: Vec<(String, TreeEntryKind)> = tree
        .root_entries()
        .iter()
        .map(|node| (node.name.clone(), node.kind))
        .collect();
    assert_eq!(
        roots,
        vec![
            ("docs".into(), TreeEntryKind::Directory),
            ("src".into(), TreeEntryKind::Directory),
            ("tests".into(), TreeEntryKind::Directory),
            ("README.md".into(), TreeEntryKind::File),
        ]
    );

    let src = tree
        .root_entries()
        .iter()
        .find(|node| node.path == "src")
        .expect("src directory");
    let src_children: Vec<(String, TreeEntryKind)> = src
        .children
        .iter()
        .map(|node| (node.name.clone(), node.kind))
        .collect();
    assert_eq!(
        src_children,
        vec![
            ("nested".into(), TreeEntryKind::Directory),
            ("lib.rs".into(), TreeEntryKind::File),
            ("main.rs".into(), TreeEntryKind::File),
        ]
    );
    assert_eq!(
        visible_paths(&tree),
        vec!["docs", "src", "tests", "README.md"]
    );
    assert!(!visible_paths(&tree).iter().any(|path| path.contains('/')));
    assert!(tree
        .root_entries()
        .iter()
        .all(|node| node.path != "untracked.txt"));
}

#[test]
fn keyboard_navigates_expands_collapses_and_selects() {
    let (_fixture, project) = sample_project();
    let mut tree = ProjectTree::from_project(&project);

    assert!(tree.handle_key(TreeKey::ArrowDown));
    assert_eq!(tree.selected_path(), Some("docs"));
    assert_eq!(tree.focused_path(), Some("docs"));

    assert!(tree.handle_key(TreeKey::ArrowDown));
    assert_eq!(tree.selected_path(), Some("src"));

    assert!(tree.handle_key(TreeKey::ArrowRight));
    assert!(tree.is_expanded("src"));
    assert_eq!(
        visible_paths(&tree),
        vec![
            "docs",
            "src",
            "src/nested",
            "src/lib.rs",
            "src/main.rs",
            "tests",
            "README.md"
        ]
    );
    assert_eq!(tree.selected_path(), Some("src"));

    assert!(tree.handle_key(TreeKey::ArrowRight));
    assert_eq!(tree.selected_path(), Some("src/nested"));

    assert!(tree.handle_key(TreeKey::Enter));
    assert!(tree.is_expanded("src/nested"));
    assert!(visible_paths(&tree).contains(&"src/nested/mod.rs".to_string()));

    assert!(tree.handle_key(TreeKey::ArrowDown));
    assert_eq!(tree.selected_path(), Some("src/nested/mod.rs"));

    assert!(tree.handle_key(TreeKey::ArrowLeft));
    assert_eq!(tree.selected_path(), Some("src/nested"));

    assert!(tree.handle_key(TreeKey::ArrowLeft));
    assert!(!tree.is_expanded("src/nested"));
    assert_eq!(tree.selected_path(), Some("src/nested"));

    assert!(tree.handle_key(TreeKey::ArrowLeft));
    assert_eq!(tree.selected_path(), Some("src"));

    assert!(tree.handle_key(TreeKey::ArrowLeft));
    assert!(!tree.is_expanded("src"));
    assert_eq!(
        visible_paths(&tree),
        vec!["docs", "src", "tests", "README.md"]
    );
}

#[test]
fn filter_preserves_ancestors_of_matches() {
    let (_fixture, project) = sample_project();
    let mut tree = ProjectTree::from_project(&project);

    tree.set_filter("mod.rs");
    assert_eq!(
        visible_paths(&tree),
        vec!["src", "src/nested", "src/nested/mod.rs"]
    );
    let rows = tree.visible_rows();
    assert_eq!(rows[0].kind, TreeEntryKind::Directory);
    assert_eq!(rows[0].depth, 0);
    assert!(rows[0].expanded);
    assert_eq!(rows[1].path, "src/nested");
    assert_eq!(rows[1].depth, 1);
    assert_eq!(rows[2].path, "src/nested/mod.rs");
    assert_eq!(rows[2].kind, TreeEntryKind::File);
    assert_eq!(rows[2].depth, 2);
    assert!(!visible_paths(&tree).contains(&"src/lib.rs".to_string()));
    assert!(!visible_paths(&tree).contains(&"README.md".to_string()));

    tree.set_filter("LIB.RS");
    assert_eq!(visible_paths(&tree), vec!["src", "src/lib.rs"]);
}

#[test]
fn reveal_selects_path_and_expands_ancestors() {
    let (_fixture, project) = sample_project();
    let mut tree = ProjectTree::from_project(&project);

    assert!(tree.reveal("src/nested/mod.rs"));
    assert_eq!(tree.selected_path(), Some("src/nested/mod.rs"));
    assert!(tree.is_expanded("src"));
    assert!(tree.is_expanded("src/nested"));
    assert!(visible_paths(&tree).contains(&"src/nested/mod.rs".to_string()));
    assert!(!tree.reveal("does/not/exist.rs"));
}

#[test]
fn expansion_and_filter_state_persists_per_project() {
    let (_fixture, project) = sample_project();
    let mut other = GitFixture::new();
    other.commit("other", &[("src/lib.rs", "// other\n")]);
    let other_project = Project::open(other.path()).expect("open other project");

    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("subsurface.db");
    let store = SqliteStore::open(&db_path).expect("store");

    let mut tree = ProjectTree::from_project(&project);
    tree.set_filter("lib.rs");
    tree.select("src/lib.rs");
    store
        .save_tree_state(tree.project_path(), &tree.snapshot_state())
        .expect("save tree state");

    let mut other_tree = ProjectTree::from_project(&other_project);
    other_tree.expand("src");
    store
        .save_tree_state(other_tree.project_path(), &other_tree.snapshot_state())
        .expect("save other tree state");
    drop(store);

    let reopened = SqliteStore::open(&db_path).expect("reopen store");
    let restored = reopened
        .load_tree_state(&project.root_path)
        .expect("load tree state")
        .expect("saved state");
    assert_eq!(restored.filter, "lib.rs");
    assert_eq!(restored.selected_path.as_deref(), Some("src/lib.rs"));
    assert!(restored.expanded_paths.contains(&"src".to_string()));

    let mut restored_tree = ProjectTree::from_project(&project);
    restored_tree.apply_state(&restored);
    assert_eq!(restored_tree.filter(), "lib.rs");
    assert_eq!(restored_tree.selected_path(), Some("src/lib.rs"));
    assert_eq!(visible_paths(&restored_tree), vec!["src", "src/lib.rs"]);

    let other_state = reopened
        .load_tree_state(&other_project.root_path)
        .expect("load other state")
        .expect("other saved state");
    assert_eq!(other_state.filter, "");
    assert!(other_state.expanded_paths.contains(&"src".to_string()));
    assert_ne!(other_state.selected_path, restored.selected_path);
}
