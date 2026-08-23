use std::process::Command;
use serde::{Deserialize, Serialize};
use crate::excavate::TimelineEntry;
use crate::site::Site;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReceiptKind {
    ClosedIssue,
    DependencyUpgraded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StalenessReceipt {
    pub kind: ReceiptKind,
    pub receipt_text: String,
    pub commit_sha: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StalenessStatus {
    Stale { receipt: StalenessReceipt },
    Active,
}

/// Checks if any workaround in the timeline has a concrete receipt saying it is dead.
/// Invariant: Age, churn, and TODO comments are NOT receipts.
pub fn detect_staleness(site: &Site, file_path: &str, timeline: &[TimelineEntry]) -> StalenessStatus {
    let mut referenced_issues = Vec::new();
    for entry in timeline {
        let msg = &entry.message;
        for word in msg.split(|c: char| !c.is_alphanumeric() && c != '#') {
            if word.starts_with('#') && word.len() > 1 {
                if let Ok(num) = word[1..].parse::<u32>() {
                    if !referenced_issues.contains(&num) {
                        referenced_issues.push(num);
                    }
                }
            }
        }
    }

    if referenced_issues.is_empty() {
        return StalenessStatus::Active;
    }

    let log_output = Command::new("git")
        .current_dir(&site.root_path)
        .args(["log", "--format=%H%x00%s%x00%b", "--all"])
        .output();

    if let Ok(out) = log_output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\0').collect();
            if parts.len() >= 2 {
                let commit_sha = parts[0].trim().to_string();
                let subject = parts[1].trim();
                let body = if parts.len() > 2 { parts[2].trim() } else { "" };
                let full = format!("{} {}", subject, body).to_lowercase();

                for issue_num in &referenced_issues {
                    let issue_tag = format!("#{}", issue_num);
                    if full.contains(&issue_tag)
                        && (full.contains("close")
                            || full.contains("closed")
                            || full.contains("closes")
                            || full.contains("fix")
                            || full.contains("fixed")
                            || full.contains("fixes")
                            || full.contains("resolve")
                            || full.contains("resolved")
                            || full.contains("upgrade"))
                    {
                        return StalenessStatus::Stale {
                            receipt: StalenessReceipt {
                                kind: ReceiptKind::ClosedIssue,
                                receipt_text: format!(
                                    "Linked issue #{} was resolved/closed in commit {}: '{}'",
                                    issue_num,
                                    &commit_sha[..7.min(commit_sha.len())],
                                    subject
                                ),
                                commit_sha,
                                file_path: file_path.to_string(),
                            },
                        };
                    }
                }
            }
        }
    }

    StalenessStatus::Active
}
