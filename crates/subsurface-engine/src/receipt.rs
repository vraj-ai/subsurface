use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::candidate::{
    fingerprint_project, prepare_candidate, CandidateEdit, CandidateError, GitFingerprint,
    PreparedCandidate,
};
use crate::command::{
    CommandAllowlist, CommandOutcome, IsolatedCommand, IsolatedRunner, IsolationError,
    ResourceBounds,
};
use crate::project::Project;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReceiptError {
    #[error("unknown queued candidate: {0}")]
    UnknownCandidate(String),
    #[error(transparent)]
    Candidate(#[from] CandidateError),
    #[error(transparent)]
    Isolation(#[from] IsolationError),
}

/// A named check run against a disposable clone. Avoids "test result".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub command: IsolatedCommand,
}

impl Check {
    pub fn new(name: impl Into<String>, command: IsolatedCommand) -> Self {
        Self {
            name: name.into(),
            command,
        }
    }
}

/// One check as measured on a clone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckRecord {
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub proved: bool,
    pub outcome: CommandOutcome,
    pub detail: String,
}

/// Baseline-versus-candidate measurement of one check.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckComparison {
    pub name: String,
    pub baseline: CheckRecord,
    pub candidate: CheckRecord,
}

/// Whether the prepared change improved the measured checks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptVerdict {
    Improved,
    Failed,
}

/// Baseline-to-candidate comparison: what improved, which checks prove it,
/// and which risks or failures remain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImprovementReceipt {
    pub base_commit: String,
    pub improved: Vec<String>,
    pub proving_checks: Vec<String>,
    pub remaining: Vec<String>,
    pub comparisons: Vec<CheckComparison>,
    pub verdict: ReceiptVerdict,
    pub project_fingerprint: GitFingerprint,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueueStatus {
    Queued,
    Prepared,
    Failed,
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueuedCandidate {
    pub id: String,
    pub status: QueueStatus,
    pub receipt: Option<ImprovementReceipt>,
}

/// In-memory queue of prepared changes. Failed candidates remain queued
/// rather than mutating or resolving the active Project.
#[derive(Debug, Default)]
pub struct ImprovementQueue {
    items: Vec<QueuedCandidate>,
}

impl ImprovementQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&mut self, id: impl Into<String>) -> &QueuedCandidate {
        self.items.push(QueuedCandidate {
            id: id.into(),
            status: QueueStatus::Queued,
            receipt: None,
        });
        self.items.last().expect("just pushed")
    }

    pub fn get(&self, id: &str) -> Option<&QueuedCandidate> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn is_queued(&self, id: &str) -> bool {
        self.items.iter().any(|item| item.id == id)
    }

    pub fn is_resolved(&self, id: &str) -> bool {
        !self.is_queued(id)
    }

    /// Measure the Project at HEAD, prepare the candidate in a disposable
    /// clone, compare, and attach an Improvement Receipt. A Failed verdict
    /// leaves the item queued.
    pub fn compare(
        &mut self,
        id: &str,
        project: &Project,
        edits: &[CandidateEdit],
        checks: &[Check],
        allowlist: CommandAllowlist,
        bounds: ResourceBounds,
    ) -> Result<ImprovementReceipt, ReceiptError> {
        if !self.is_queued(id) {
            return Err(ReceiptError::UnknownCandidate(id.to_string()));
        }

        let receipt = compare_baseline_and_candidate(project, edits, checks, allowlist, bounds)?;

        let item = self
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .expect("queued item");
        item.status = match receipt.verdict {
            ReceiptVerdict::Improved => QueueStatus::Verified,
            ReceiptVerdict::Failed => QueueStatus::Failed,
        };
        item.receipt = Some(receipt.clone());
        Ok(receipt)
    }
}

/// Prepare a baseline measurement, then a candidate in a disposable clone,
/// and emit an Improvement Receipt. The active Project is not modified.
pub fn compare_baseline_and_candidate(
    project: &Project,
    edits: &[CandidateEdit],
    checks: &[Check],
    allowlist: CommandAllowlist,
    bounds: ResourceBounds,
) -> Result<ImprovementReceipt, ReceiptError> {
    let before = fingerprint_project(project)?;

    let baseline_clone = prepare_candidate(project, &[])?;
    let baseline = measure(
        project,
        &baseline_clone,
        checks,
        allowlist.clone(),
        bounds.clone(),
    )?;

    let candidate_clone = prepare_candidate(project, edits)?;
    let candidate = measure(project, &candidate_clone, checks, allowlist, bounds)?;

    let after = fingerprint_project(project)?;
    if after != before {
        return Err(ReceiptError::Candidate(CandidateError::ProjectMutated));
    }

    Ok(build_receipt(before, baseline, candidate))
}

fn measure(
    project: &Project,
    prepared: &PreparedCandidate,
    checks: &[Check],
    allowlist: CommandAllowlist,
    bounds: ResourceBounds,
) -> Result<Vec<CheckRecord>, ReceiptError> {
    let runner = IsolatedRunner::new(
        project.clone(),
        prepared.clone_path.clone(),
        allowlist,
        bounds,
    );
    let mut records = Vec::with_capacity(checks.len());
    for check in checks {
        let receipt = runner.run(&check.command)?;
        let proved = receipt.outcome == CommandOutcome::Succeeded;
        let detail = if receipt.stderr.trim().is_empty() {
            receipt.stdout.trim().to_string()
        } else if receipt.stdout.trim().is_empty() {
            receipt.stderr.trim().to_string()
        } else {
            format!("{}\n{}", receipt.stdout.trim(), receipt.stderr.trim())
        };
        records.push(CheckRecord {
            name: check.name.clone(),
            program: receipt.program,
            args: receipt.args,
            proved,
            outcome: receipt.outcome,
            detail,
        });
    }
    Ok(records)
}

fn build_receipt(
    fingerprint: GitFingerprint,
    baseline: Vec<CheckRecord>,
    candidate: Vec<CheckRecord>,
) -> ImprovementReceipt {
    let mut improved = Vec::new();
    let mut proving_checks = Vec::new();
    let mut remaining = Vec::new();
    let mut comparisons = Vec::new();

    for (baseline_record, candidate_record) in baseline.into_iter().zip(candidate) {
        let name = candidate_record.name.clone();
        if candidate_record.proved {
            proving_checks.push(name.clone());
            if !baseline_record.proved {
                improved.push(format!(
                    "{name} now proves the prepared change; the baseline did not"
                ));
            }
        } else {
            remaining.push(format!(
                "{name} did not prove the prepared change ({:?})",
                candidate_record.outcome
            ));
        }

        comparisons.push(CheckComparison {
            name,
            baseline: baseline_record,
            candidate: candidate_record,
        });
    }

    let verdict = if remaining.is_empty() && !improved.is_empty() {
        ReceiptVerdict::Improved
    } else {
        ReceiptVerdict::Failed
    };

    ImprovementReceipt {
        base_commit: fingerprint.head.clone(),
        improved,
        proving_checks,
        remaining,
        comparisons,
        verdict,
        project_fingerprint: fingerprint,
    }
}
