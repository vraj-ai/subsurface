use std::process::Command;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use crate::site::Site;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceKind {
    DirectDiff,
    CoCommittedTest,
    CoCommittedDoc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub commit_sha: String,
    pub author: String,
    pub timestamp: String,
    pub message: String,
    pub diff: String,
    pub lines_touched: usize,
    pub kind: EvidenceKind,
    pub is_heuristic: bool,
    pub heuristic_note: Option<String>,
    pub file_path: String,
}

#[derive(Debug, Error)]
pub enum EvidenceError {
    #[error("Git execution failed: {0}")]
    Git(String),
}

/// Walks git history for the given file and line range using git log -L and co-commit heuristics.
pub fn walk_evidence(
    site: &Site,
    file_path: &str,
    range: LineRange,
) -> Result<Vec<Evidence>, EvidenceError> {
    let line_arg = format!("{},{}:{}", range.start, range.end, file_path);

    let output = Command::new("git")
        .current_dir(&site.root_path)
        .args([
            "log",
            "-L",
            &line_arg,
            "-w",
            "--format=__SUB_COMMIT__%H%x00%an%x00%aI%x00%s%x00%b",
        ])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output();

    let Ok(output) = output else {
        return Err(EvidenceError::Git("Failed to spawn git process".into()));
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut evidence_items = Vec::new();

    let mut seen_shas = std::collections::HashSet::new();

    for chunk in stdout.split("__SUB_COMMIT__") {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }

        let parts: Vec<&str> = chunk.splitn(2, '\n').collect();
        let header_line = parts[0];
        let diff_body = if parts.len() > 1 { parts[1] } else { "" };

        let header_fields: Vec<&str> = header_line.split('\0').collect();
        if header_fields.is_empty() || header_fields[0].is_empty() {
            continue;
        }

        let sha = header_fields[0].trim().to_string();
        let author = header_fields.get(1).unwrap_or(&"").trim().to_string();
        let timestamp = header_fields.get(2).unwrap_or(&"").trim().to_string();
        let subject = header_fields.get(3).unwrap_or(&"").trim().to_string();
        let body = header_fields.get(4).unwrap_or(&"").trim().to_string();

        let full_message = if body.is_empty() {
            subject.clone()
        } else {
            format!("{}\n\n{}", subject, body)
        };

        let lines_touched = diff_body
            .lines()
            .filter(|l| l.starts_with('+') || l.starts_with('-'))
            .count()
            .max(1);

        if !seen_shas.contains(&sha) {
            seen_shas.insert(sha.clone());

            evidence_items.push(Evidence {
                commit_sha: sha.clone(),
                author: author.clone(),
                timestamp: timestamp.clone(),
                message: full_message.clone(),
                diff: diff_body.to_string(),
                lines_touched,
                kind: EvidenceKind::DirectDiff,
                is_heuristic: false,
                heuristic_note: None,
                file_path: file_path.to_string(),
            });

            // Find co-committed tests and docs
            let show_out = Command::new("git")
                .current_dir(&site.root_path)
                .args(["show", "--name-only", "--format=", &sha])
                .output();

            if let Ok(show_out) = show_out {
                let show_str = String::from_utf8_lossy(&show_out.stdout);
                for changed_file in show_str.lines() {
                    let changed_file = changed_file.trim();
                    if changed_file.is_empty() || changed_file == file_path {
                        continue;
                    }

                    if is_test_file(changed_file) {
                        evidence_items.push(Evidence {
                            commit_sha: sha.clone(),
                            author: author.clone(),
                            timestamp: timestamp.clone(),
                            message: full_message.clone(),
                            diff: String::new(),
                            lines_touched: 0,
                            kind: EvidenceKind::CoCommittedTest,
                            is_heuristic: true,
                            heuristic_note: Some(format!(
                                "Co-committed test file `{}`; heuristic linkage, not verified coverage",
                                changed_file
                            )),
                            file_path: changed_file.to_string(),
                        });
                    } else if is_doc_file(changed_file) {
                        evidence_items.push(Evidence {
                            commit_sha: sha.clone(),
                            author: author.clone(),
                            timestamp: timestamp.clone(),
                            message: full_message.clone(),
                            diff: String::new(),
                            lines_touched: 0,
                            kind: EvidenceKind::CoCommittedDoc,
                            is_heuristic: true,
                            heuristic_note: Some(format!(
                                "Co-committed doc file `{}`; heuristic linkage, not verified coverage",
                                changed_file
                            )),
                            file_path: changed_file.to_string(),
                        });
                    }
                }
            }
        }
    }

    Ok(evidence_items)
}

fn is_test_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("test")
        || lower.contains("spec")
        || lower.starts_with("tests/")
        || lower.starts_with("test/")
}

fn is_doc_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".md")
        || lower.ends_with(".rst")
        || lower.ends_with(".txt")
        || lower.starts_with("docs/")
        || lower.starts_with("doc/")
}
