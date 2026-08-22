use serde::{Deserialize, Serialize};
use crate::evidence::{Evidence, EvidenceKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub max_evidence_items: usize,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_evidence_items: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExcludedEvidence {
    pub commit_sha: String,
    pub message_summary: String,
    pub reason: String,
    pub lines_touched: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetResult {
    pub included: Vec<Evidence>,
    pub excluded: Vec<ExcludedEvidence>,
    pub total_considered: usize,
}

/// Ranks candidate Evidence and caps what is sent to a provider, recording what was excluded.
pub fn rank_and_budget_evidence(
    candidates: &[Evidence],
    config: &BudgetConfig,
) -> BudgetResult {
    let total_considered = candidates.len();

    let mut ranked = candidates.to_vec();
    ranked.sort_by(|a, b| {
        let kind_score = |k: &EvidenceKind| match k {
            EvidenceKind::DirectDiff => 2,
            EvidenceKind::CoCommittedTest => 1,
            EvidenceKind::CoCommittedDoc => 1,
        };

        kind_score(&b.kind)
            .cmp(&kind_score(&a.kind))
            .then_with(|| b.lines_touched.cmp(&a.lines_touched))
            .then_with(|| a.commit_sha.cmp(&b.commit_sha))
    });

    let mut included = Vec::new();
    let mut excluded = Vec::new();

    for (idx, item) in ranked.into_iter().enumerate() {
        if idx < config.max_evidence_items {
            included.push(item);
        } else {
            let first_line = item.message.lines().next().unwrap_or("").to_string();
            excluded.push(ExcludedEvidence {
                commit_sha: item.commit_sha,
                message_summary: first_line,
                reason: format!(
                    "Exceeded max evidence item budget (cap = {})",
                    config.max_evidence_items
                ),
                lines_touched: item.lines_touched,
            });
        }
    }

    BudgetResult {
        included,
        excluded,
        total_considered,
    }
}
