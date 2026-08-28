use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::project::Project;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeEntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TreeNode {
    pub path: String,
    pub name: String,
    pub kind: TreeEntryKind,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisibleRow {
    pub path: String,
    pub name: String,
    pub kind: TreeEntryKind,
    pub depth: usize,
    pub expanded: bool,
    pub selected: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKey {
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Enter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProjectTreeState {
    pub expanded_paths: Vec<String>,
    pub filter: String,
    pub selected_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectTree {
    project_path: PathBuf,
    children: Vec<TreeNode>,
    expanded: BTreeSet<String>,
    filter: String,
    selected: Option<String>,
    focused: Option<String>,
}

impl ProjectTree {
    pub fn from_project(project: &Project) -> Self {
        Self::from_tracked_files(project.root_path.clone(), &project.tracked_files)
    }

    pub fn from_tracked_files(project_path: PathBuf, files: &[String]) -> Self {
        let mut builder = Builder::default();
        for file in files {
            let parts: Vec<&str> = file.split('/').filter(|part| !part.is_empty()).collect();
            if !parts.is_empty() {
                builder.insert(&parts);
            }
        }
        Self {
            project_path,
            children: builder.into_nodes(""),
            expanded: BTreeSet::new(),
            filter: String::new(),
            selected: None,
            focused: None,
        }
    }

    pub fn project_path(&self) -> &Path {
        &self.project_path
    }

    pub fn root_entries(&self) -> &[TreeNode] {
        &self.children
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn selected_path(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn focused_path(&self) -> Option<&str> {
        self.focused.as_deref()
    }

    pub fn is_expanded(&self, path: &str) -> bool {
        self.expanded.contains(path)
    }

    pub fn snapshot_state(&self) -> ProjectTreeState {
        ProjectTreeState {
            expanded_paths: self.expanded.iter().cloned().collect(),
            filter: self.filter.clone(),
            selected_path: self.selected.clone(),
        }
    }

    pub fn apply_state(&mut self, state: &ProjectTreeState) {
        self.expanded = state
            .expanded_paths
            .iter()
            .filter(|path| self.contains_path(path))
            .cloned()
            .collect();
        if let Some(path) = state.selected_path.as_deref() {
            if self.contains_path(path) {
                self.selected = Some(path.to_string());
                self.focused = Some(path.to_string());
            }
        }
        self.set_filter(state.filter.clone());
    }

    pub fn set_filter(&mut self, filter: impl Into<String>) {
        self.filter = filter.into();
        if self.filter.is_empty() {
            return;
        }
        let mut to_expand = Vec::new();
        collect_match_ancestors(&self.children, &self.filter, &mut to_expand);
        self.expanded.extend(to_expand);
    }

    pub fn expand(&mut self, path: &str) -> bool {
        if !self.is_directory(path) {
            return false;
        }
        self.expanded.insert(path.to_string())
    }

    pub fn collapse(&mut self, path: &str) -> bool {
        self.expanded.remove(path)
    }

    pub fn toggle_expanded(&mut self, path: &str) -> bool {
        if self.expanded.contains(path) {
            self.collapse(path)
        } else {
            self.expand(path)
        }
    }

    pub fn select(&mut self, path: &str) -> bool {
        if !self.contains_path(path) {
            return false;
        }
        self.selected = Some(path.to_string());
        self.focused = Some(path.to_string());
        true
    }

    pub fn reveal(&mut self, path: &str) -> bool {
        if !self.contains_path(path) {
            return false;
        }
        for ancestor in ancestor_paths(path) {
            self.expanded.insert(ancestor);
        }
        self.select(path)
    }

    pub fn handle_key(&mut self, key: TreeKey) -> bool {
        let rows = self.visible_rows();
        if rows.is_empty() {
            return false;
        }
        let current = self
            .focused
            .as_ref()
            .and_then(|path| rows.iter().position(|row| &row.path == path));

        match key {
            TreeKey::ArrowDown => {
                let idx = current.map(|i| (i + 1).min(rows.len() - 1)).unwrap_or(0);
                self.focus_row(&rows[idx].path)
            }
            TreeKey::ArrowUp => {
                let idx = current
                    .map(|i| i.saturating_sub(1))
                    .unwrap_or(rows.len() - 1);
                self.focus_row(&rows[idx].path)
            }
            TreeKey::ArrowRight => {
                let Some(idx) = current else {
                    return self.focus_row(&rows[0].path);
                };
                let path = rows[idx].path.clone();
                let kind = rows[idx].kind;
                let expanded = rows[idx].expanded;
                let depth = rows[idx].depth;
                if kind == TreeEntryKind::Directory && !expanded {
                    self.expanded.insert(path);
                    true
                } else if kind == TreeEntryKind::Directory
                    && expanded
                    && rows.get(idx + 1).is_some_and(|child| child.depth > depth)
                {
                    let child_path = rows[idx + 1].path.clone();
                    self.focus_row(&child_path)
                } else {
                    false
                }
            }
            TreeKey::ArrowLeft => {
                let Some(idx) = current else {
                    return false;
                };
                let path = rows[idx].path.clone();
                let kind = rows[idx].kind;
                let depth = rows[idx].depth;
                if kind == TreeEntryKind::Directory && self.expanded.contains(&path) {
                    self.expanded.remove(&path);
                    true
                } else if depth > 0 {
                    if let Some(parent) = parent_path(&path) {
                        self.focus_row(&parent)
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            TreeKey::Enter => {
                let Some(idx) = current else {
                    return self.focus_row(&rows[0].path);
                };
                let path = rows[idx].path.clone();
                if rows[idx].kind == TreeEntryKind::Directory {
                    self.toggle_expanded(&path)
                } else {
                    self.focus_row(&path)
                }
            }
        }
    }

    pub fn visible_rows(&self) -> Vec<VisibleRow> {
        let mut rows = Vec::new();
        self.collect_visible(&self.children, 0, &mut rows);
        rows
    }

    fn collect_visible(&self, nodes: &[TreeNode], depth: usize, rows: &mut Vec<VisibleRow>) {
        for node in nodes {
            if !self.is_visible(node) {
                continue;
            }
            let expanded = node.kind == TreeEntryKind::Directory && self.is_display_expanded(node);
            rows.push(VisibleRow {
                path: node.path.clone(),
                name: node.name.clone(),
                kind: node.kind,
                depth,
                expanded,
                selected: self.selected.as_deref() == Some(node.path.as_str()),
                focused: self.focused.as_deref() == Some(node.path.as_str()),
            });
            if expanded {
                self.collect_visible(&node.children, depth + 1, rows);
            }
        }
    }

    fn is_visible(&self, node: &TreeNode) -> bool {
        self.filter.trim().is_empty() || subtree_matches(node, &self.filter)
    }

    fn is_display_expanded(&self, node: &TreeNode) -> bool {
        self.expanded.contains(&node.path)
    }

    fn contains_path(&self, path: &str) -> bool {
        find_node(&self.children, path).is_some()
    }

    fn is_directory(&self, path: &str) -> bool {
        find_node(&self.children, path).is_some_and(|node| node.kind == TreeEntryKind::Directory)
    }

    fn focus_row(&mut self, path: &str) -> bool {
        let changed =
            self.focused.as_deref() != Some(path) || self.selected.as_deref() != Some(path);
        self.focused = Some(path.to_string());
        self.selected = Some(path.to_string());
        changed
    }
}

#[derive(Default)]
struct Builder {
    dirs: BTreeMap<String, Builder>,
    files: BTreeMap<String, ()>,
}

impl Builder {
    fn insert(&mut self, parts: &[&str]) {
        match parts {
            [] => {}
            [name] => {
                self.files.insert((*name).to_string(), ());
            }
            [name, rest @ ..] => {
                self.dirs
                    .entry((*name).to_string())
                    .or_default()
                    .insert(rest);
            }
        }
    }

    fn into_nodes(self, prefix: &str) -> Vec<TreeNode> {
        let mut dirs: Vec<TreeNode> = self
            .dirs
            .into_iter()
            .map(|(name, child)| {
                let path = join_path(prefix, &name);
                TreeNode {
                    path: path.clone(),
                    name,
                    kind: TreeEntryKind::Directory,
                    children: child.into_nodes(&path),
                }
            })
            .collect();
        let mut files: Vec<TreeNode> = self
            .files
            .into_iter()
            .map(|(name, _)| TreeNode {
                path: join_path(prefix, &name),
                name,
                kind: TreeEntryKind::File,
                children: Vec::new(),
            })
            .collect();
        dirs.sort_by(|a, b| cmp_name(&a.name, &b.name));
        files.sort_by(|a, b| cmp_name(&a.name, &b.name));
        dirs.append(&mut files);
        dirs
    }
}

fn join_path(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn cmp_name(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

fn parent_path(path: &str) -> Option<String> {
    path.rsplit_once('/').map(|(parent, _)| parent.to_string())
}

fn ancestor_paths(path: &str) -> Vec<String> {
    let mut ancestors = Vec::new();
    let mut current = path;
    while let Some(parent) = parent_path(current) {
        ancestors.push(parent.clone());
        current = ancestors.last().expect("just pushed");
    }
    ancestors.reverse();
    ancestors
}

fn find_node<'a>(nodes: &'a [TreeNode], path: &str) -> Option<&'a TreeNode> {
    for node in nodes {
        if node.path == path {
            return Some(node);
        }
        if let Some(found) = find_node(&node.children, path) {
            return Some(found);
        }
    }
    None
}

fn path_matches(path: &str, filter: &str) -> bool {
    let needle = filter.trim().to_lowercase();
    !needle.is_empty() && path.to_lowercase().contains(&needle)
}

fn subtree_matches(node: &TreeNode, filter: &str) -> bool {
    path_matches(&node.path, filter)
        || node
            .children
            .iter()
            .any(|child| subtree_matches(child, filter))
}

fn collect_match_ancestors(nodes: &[TreeNode], filter: &str, out: &mut Vec<String>) {
    for node in nodes {
        if node.kind != TreeEntryKind::Directory {
            continue;
        }
        if node
            .children
            .iter()
            .any(|child| subtree_matches(child, filter))
        {
            out.push(node.path.clone());
            collect_match_ancestors(&node.children, filter, out);
        }
    }
}
