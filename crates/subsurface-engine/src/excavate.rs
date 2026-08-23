use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::budget::{rank_and_budget_evidence, BudgetConfig, ExcludedEvidence};
use crate::confidence::{assign_confidence, Confidence};
use crate::evidence::{walk_evidence, Evidence, EvidenceKind, LineRange};
use crate::provider::Provider;
use crate::site::Site;
use crate::staleness::{detect_staleness, StalenessStatus};

#[derive(Debug, Error)]
pub enum ExcavateError {
    #[error("Failed to read current file content: {0}")]
    Io(#[from] std::io::Error),
    #[error("Evidence walk failed: {0}")]
    Evidence(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineEntry {
    pub commit_sha: String,
    pub author: String,
    pub timestamp: String,
    pub message: String,
    pub diff_hunk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WhatWhen {
    pub first_introduced_commit: Option<String>,
    pub first_introduced_author: Option<String>,
    pub first_introduced_timestamp: Option<String>,
    pub evolution_summary: String,
    pub timeline: Vec<TimelineEntry>,
    pub related_tests: Vec<String>,
    pub related_docs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceCitation {
    pub claim: String,
    pub commit_sha: String,
    pub citation_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WhyRationale {
    pub rationale: String,
    pub confidence: Confidence,
    pub confidence_explanation: String,
    pub evidence_citations: Vec<EvidenceCitation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BudgetSummary {
    pub total_considered: usize,
    pub included_count: usize,
    pub excluded: Vec<ExcludedEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding {
    #[serde(rename = "project_path", alias = "site_path")]
    pub site_path: PathBuf,
    pub file_path: String,
    pub line_range: LineRange,
    pub current_code: String,
    pub what_when: WhatWhen,
    pub why: WhyRationale,
    pub budget_summary: BudgetSummary,
    pub staleness: StalenessStatus,
}

/// The single seam through which UI, MCP server, and Site Report all operate.
pub fn excavate(
    site: &Site,
    file_path: &str,
    line_range: LineRange,
    provider: Arc<dyn Provider>,
) -> Result<Finding, ExcavateError> {
    // 1. Read current code for the selection
    let full_path = site.root_path.join(file_path);
    let full_file_content = fs::read_to_string(&full_path).unwrap_or_default();
    let file_lines: Vec<&str> = full_file_content.lines().collect();

    let start_idx = line_range.start.saturating_sub(1);
    let end_idx = line_range.end.min(file_lines.len());
    let current_code = if start_idx < end_idx && start_idx < file_lines.len() {
        file_lines[start_idx..end_idx].join("\n")
    } else {
        String::new()
    };

    // 2. Walk candidate Evidence locally from git
    let raw_evidence = walk_evidence(site, file_path, line_range)
        .map_err(|e| ExcavateError::Evidence(e.to_string()))?;

    // 3. Budget and rank Evidence
    let budget_result = rank_and_budget_evidence(&raw_evidence, &BudgetConfig::default());
    let included_evidence = budget_result.included;

    // 4. Assign rule-based Confidence
    let confidence = assign_confidence(&included_evidence);

    // 5. Build What/When half (pure local git, 0 provider involvement)
    let direct_commits: Vec<&Evidence> = included_evidence
        .iter()
        .filter(|e| e.kind == EvidenceKind::DirectDiff)
        .collect();

    let oldest_commit = direct_commits
        .last()
        .copied()
        .or_else(|| included_evidence.last());
    let first_introduced_commit = oldest_commit.map(|e| e.commit_sha.clone());
    let first_introduced_author = oldest_commit.map(|e| e.author.clone());
    let first_introduced_timestamp = oldest_commit.map(|e| e.timestamp.clone());

    let timeline: Vec<TimelineEntry> = direct_commits
        .iter()
        .map(|e| TimelineEntry {
            commit_sha: e.commit_sha.clone(),
            author: e.author.clone(),
            timestamp: e.timestamp.clone(),
            message: e.message.clone(),
            diff_hunk: e.diff.clone(),
        })
        .collect();

    let mut related_tests = Vec::new();
    let mut related_docs = Vec::new();

    for ev in &included_evidence {
        if ev.kind == EvidenceKind::CoCommittedTest && !related_tests.contains(&ev.file_path) {
            related_tests.push(ev.file_path.clone());
        } else if ev.kind == EvidenceKind::CoCommittedDoc && !related_docs.contains(&ev.file_path) {
            related_docs.push(ev.file_path.clone());
        }
    }

    let evolution_summary = if timeline.is_empty() {
        "No direct change history recorded in git for this selection.".to_string()
    } else {
        format!(
            "Modified across {} commit(s) since first introduced.",
            timeline.len()
        )
    };

    let what_when = WhatWhen {
        first_introduced_commit,
        first_introduced_author,
        first_introduced_timestamp,
        evolution_summary,
        timeline,
        related_tests,
        related_docs,
    };

    // 6. Detect receipt-gated staleness
    let staleness = detect_staleness(site, file_path, &what_when.timeline);

    // 7. Build Why half (evidence-gated)
    let why = match confidence {
        Confidence::None => WhyRationale {
            rationale: "No recorded rationale found for this selection. The repository history records changes to this code, but no commit message, document, or test states why."
                .to_string(),
            confidence: Confidence::None,
            confidence_explanation:
                "All commits in history have empty or non-descriptive messages ('fix'/'wip') and no co-committed tests/docs."
                    .to_string(),
            evidence_citations: Vec::new(),
        },
        Confidence::Stated | Confidence::Inferred => {
            let mut prompt_evidence = String::new();
            let mut citations = Vec::new();

            for ev in &included_evidence {
                prompt_evidence.push_str(&format!(
                    "Commit: {}\nAuthor: {}\nDate: {}\nMessage: {}\n\n",
                    ev.commit_sha, ev.author, ev.timestamp, ev.message
                ));

                let first_msg_line = ev.message.lines().next().unwrap_or("").to_string();
                citations.push(EvidenceCitation {
                    claim: format!(
                        "Commit {}: {}",
                        &ev.commit_sha[..7.min(ev.commit_sha.len())],
                        first_msg_line
                    ),
                    commit_sha: ev.commit_sha.clone(),
                    citation_label: format!(
                        "Commit {}",
                        &ev.commit_sha[..7.min(ev.commit_sha.len())]
                    ),
                });
            }

            let prompt = format!(
                "You are Subsurface. Given the code selection and the recorded git evidence below, explain why this code exists in plain concise sentences. Only cite reasons supported directly by the evidence.\n\nCode selection:\n```\n{}\n```\n\nEvidence:\n{}",
                current_code, prompt_evidence
            );

            let rationale = match provider.complete(&prompt) {
                Ok(resp) => resp,
                Err(err) => format!(
                    "Inference provider unavailable ({}) - history What/When is detailed below.",
                    err
                ),
            };

            let confidence_explanation = match confidence {
                Confidence::Stated => {
                    "Rationale explicitly stated in commit messages or repository documentation."
                        .to_string()
                }
                Confidence::Inferred => {
                    "Rationale inferred from contextual co-committed changes and tests.".to_string()
                }
                Confidence::None => unreachable!(),
            };

            WhyRationale {
                rationale,
                confidence,
                confidence_explanation,
                evidence_citations: citations,
            }
        }
    };

    let budget_summary = BudgetSummary {
        total_considered: budget_result.total_considered,
        included_count: included_evidence.len(),
        excluded: budget_result.excluded,
    };

    Ok(Finding {
        site_path: site.root_path.clone(),
        file_path: file_path.to_string(),
        line_range,
        current_code,
        what_when,
        why,
        budget_summary,
        staleness,
    })
}
