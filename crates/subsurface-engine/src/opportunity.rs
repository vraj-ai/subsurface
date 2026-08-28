use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::report::{SiteReport, SiteReportCategory};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityState {
    Detected,
    Prepared,
    Verified,
    Failed,
    Published,
    Dismissed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityCategory {
    DeadWorkaround,
    MissingRationale,
    TestGap,
    ModelProposed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptSource {
    Tooling,
    Heuristic,
    ModelProposed,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImprovementReceipt {
    pub source: ReceiptSource,
    pub summary: String,
    pub reference: String,
}

impl ImprovementReceipt {
    pub fn new(
        source: ReceiptSource,
        summary: impl Into<String>,
        reference: impl Into<String>,
    ) -> Self {
        Self {
            source,
            summary: summary.into(),
            reference: reference.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpportunityRank {
    pub impact: Impact,
    pub verification: Verification,
    pub expected_grade_improvement: u8,
    pub effort: Effort,
    pub age_days: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Impact {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Verification {
    NotRun,
    Failed,
    Verified,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpportunityEvent {
    pub state: OpportunityState,
    pub at: String,
    pub receipt: Option<ImprovementReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Opportunity {
    pub id: String,
    pub finding_ids: Vec<String>,
    pub file_path: String,
    pub category: OpportunityCategory,
    pub state: OpportunityState,
    pub rank: OpportunityRank,
    pub events: Vec<OpportunityEvent>,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid Opportunity transition from {from:?} to {to:?}")]
pub struct TransitionError {
    pub from: OpportunityState,
    pub to: OpportunityState,
}

impl Opportunity {
    pub fn detected(
        id: impl Into<String>,
        finding_id: impl Into<String>,
        file_path: impl Into<String>,
        category: OpportunityCategory,
        mut rank: OpportunityRank,
        at: impl Into<String>,
        receipt: ImprovementReceipt,
    ) -> Self {
        rank.verification = Verification::NotRun;
        Self {
            id: id.into(),
            finding_ids: vec![finding_id.into()],
            file_path: file_path.into(),
            category,
            state: OpportunityState::Detected,
            rank,
            events: vec![OpportunityEvent {
                state: OpportunityState::Detected,
                at: at.into(),
                receipt: Some(receipt),
            }],
        }
    }

    pub fn transition(
        &mut self,
        next: OpportunityState,
        at: impl Into<String>,
        receipt: ImprovementReceipt,
    ) -> Result<(), TransitionError> {
        let allowed = matches!(
            (self.state, next),
            (OpportunityState::Detected, OpportunityState::Prepared)
                | (OpportunityState::Detected, OpportunityState::Dismissed)
                | (OpportunityState::Prepared, OpportunityState::Verified)
                | (OpportunityState::Prepared, OpportunityState::Failed)
                | (OpportunityState::Prepared, OpportunityState::Dismissed)
                | (OpportunityState::Verified, OpportunityState::Published)
                | (OpportunityState::Verified, OpportunityState::Dismissed)
                | (OpportunityState::Failed, OpportunityState::Dismissed)
        );
        if !allowed {
            return Err(TransitionError {
                from: self.state,
                to: next,
            });
        }
        match next {
            OpportunityState::Verified => self.rank.verification = Verification::Verified,
            OpportunityState::Failed => self.rank.verification = Verification::Failed,
            _ => {}
        }
        self.state = next;
        self.events.push(OpportunityEvent {
            state: next,
            at: at.into(),
            receipt: Some(receipt),
        });
        Ok(())
    }
}

pub fn detect_dead_workaround(
    finding_id: &str,
    file_path: &str,
    mut receipt: ImprovementReceipt,
    rank: OpportunityRank,
    at: &str,
) -> Opportunity {
    receipt.source = ReceiptSource::Tooling;
    detected(
        finding_id,
        file_path,
        OpportunityCategory::DeadWorkaround,
        receipt,
        rank,
        at,
    )
}

pub fn detect_missing_rationale(
    finding_id: &str,
    file_path: &str,
    mut receipt: ImprovementReceipt,
    rank: OpportunityRank,
    at: &str,
) -> Opportunity {
    receipt.source = ReceiptSource::Tooling;
    detected(
        finding_id,
        file_path,
        OpportunityCategory::MissingRationale,
        receipt,
        rank,
        at,
    )
}

pub fn detect_test_gap(
    finding_id: &str,
    file_path: &str,
    heuristic_disclosure: &str,
    rank: OpportunityRank,
    at: &str,
) -> Opportunity {
    detected(
        finding_id,
        file_path,
        OpportunityCategory::TestGap,
        ImprovementReceipt::new(
            ReceiptSource::Heuristic,
            heuristic_disclosure,
            format!("heuristic:{finding_id}"),
        ),
        rank,
        at,
    )
}

pub fn model_proposed_opportunity(
    finding_id: &str,
    file_path: &str,
    proposal: &str,
    rank: OpportunityRank,
    at: &str,
) -> Opportunity {
    detected(
        finding_id,
        file_path,
        OpportunityCategory::ModelProposed,
        ImprovementReceipt::new(
            ReceiptSource::ModelProposed,
            proposal,
            format!("model-proposal:{finding_id}"),
        ),
        rank,
        at,
    )
}

pub fn opportunities_from_report(report: &SiteReport, rank: OpportunityRank) -> Vec<Opportunity> {
    report
        .entries
        .iter()
        .map(|entry| {
            let finding_id = format!(
                "{}:{}-{}@{}",
                entry.file_path, entry.line_range.start, entry.line_range.end, report.head_commit
            );
            match entry.category {
                SiteReportCategory::DeadWorkaround => detect_dead_workaround(
                    &finding_id,
                    &entry.file_path,
                    ImprovementReceipt::new(
                        ReceiptSource::Tooling,
                        &entry.description,
                        format!("git:{}", report.head_commit),
                    ),
                    rank,
                    &report.generated_at,
                ),
                SiteReportCategory::NoRationale => detect_missing_rationale(
                    &finding_id,
                    &entry.file_path,
                    ImprovementReceipt::new(
                        ReceiptSource::Tooling,
                        &entry.description,
                        format!("history:{finding_id}"),
                    ),
                    rank,
                    &report.generated_at,
                ),
                SiteReportCategory::UntestedCode => detect_test_gap(
                    &finding_id,
                    &entry.file_path,
                    &format!(
                        "{} This co-commit heuristic is not verified coverage.",
                        entry.description
                    ),
                    rank,
                    &report.generated_at,
                ),
            }
        })
        .collect()
}

pub fn order_opportunities(mut opportunities: Vec<Opportunity>) -> Vec<Opportunity> {
    opportunities.sort_by(|left, right| {
        right
            .rank
            .impact
            .cmp(&left.rank.impact)
            .then_with(|| right.rank.verification.cmp(&left.rank.verification))
            .then_with(|| {
                right
                    .rank
                    .expected_grade_improvement
                    .cmp(&left.rank.expected_grade_improvement)
            })
            .then_with(|| left.rank.effort.cmp(&right.rank.effort))
            .then_with(|| right.rank.age_days.cmp(&left.rank.age_days))
            .then_with(|| left.id.cmp(&right.id))
    });
    opportunities
}

fn detected(
    finding_id: &str,
    file_path: &str,
    category: OpportunityCategory,
    receipt: ImprovementReceipt,
    rank: OpportunityRank,
    at: &str,
) -> Opportunity {
    Opportunity::detected(
        format!("{}:{}", category_name(category), finding_id),
        finding_id,
        file_path,
        category,
        rank,
        at,
        receipt,
    )
}

fn category_name(category: OpportunityCategory) -> &'static str {
    match category {
        OpportunityCategory::DeadWorkaround => "dead-workaround",
        OpportunityCategory::MissingRationale => "missing-rationale",
        OpportunityCategory::TestGap => "test-gap",
        OpportunityCategory::ModelProposed => "model-proposed",
    }
}
