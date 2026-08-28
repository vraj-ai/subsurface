use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use thiserror::Error;

use crate::grade::{QualityDimension, QualityGrade};
use crate::opportunity::Opportunity;
use crate::project::Project;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DimensionDelta {
    pub dimension: QualityDimension,
    pub score_delta: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectAssessment {
    pub project_path: PathBuf,
    pub commit_sha: String,
    pub assessed_at: String,
    pub grade: QualityGrade,
    pub opportunities: Vec<Opportunity>,
    pub baseline_commit_sha: Option<String>,
    pub overall_delta: Option<i16>,
    pub dimension_deltas: Vec<DimensionDelta>,
}

impl ProjectAssessment {
    pub fn at_project_head(
        project: &Project,
        grade: QualityGrade,
        opportunities: Vec<Opportunity>,
        assessed_at: impl Into<String>,
        baseline: Option<&ProjectAssessment>,
    ) -> Result<Self, AssessmentError> {
        let commit_sha = project
            .head_commit
            .clone()
            .ok_or(AssessmentError::NoHeadCommit)?;
        let baseline_scores = baseline
            .map(|item| {
                item.grade
                    .dimensions
                    .iter()
                    .map(|dimension| (dimension.dimension, dimension.score))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        let dimension_deltas = grade
            .dimensions
            .iter()
            .filter_map(|dimension| {
                baseline_scores
                    .get(&dimension.dimension)
                    .map(|baseline| DimensionDelta {
                        dimension: dimension.dimension,
                        score_delta: i16::from(dimension.score) - i16::from(*baseline),
                    })
            })
            .collect();
        let overall_delta = grade
            .overall_score
            .zip(baseline.and_then(|item| item.grade.overall_score))
            .map(|(current, previous)| i16::from(current) - i16::from(previous));

        Ok(Self {
            project_path: project.root_path.clone(),
            commit_sha,
            assessed_at: assessed_at.into(),
            grade,
            opportunities,
            baseline_commit_sha: baseline.map(|item| item.commit_sha.clone()),
            overall_delta,
            dimension_deltas,
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AssessmentError {
    #[error("Project has no HEAD commit to assess")]
    NoHeadCommit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Provider,
    Command,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityPreview {
    pub kind: ActivityKind,
    pub label: String,
    pub detail: String,
}

impl ActivityPreview {
    pub fn provider(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind: ActivityKind::Provider,
            label: label.into(),
            detail: detail.into(),
        }
    }

    pub fn command(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind: ActivityKind::Command,
            label: label.into(),
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssessmentPreview {
    pub commit_sha: String,
    pub activities: Vec<ActivityPreview>,
}

impl AssessmentPreview {
    pub fn new(commit_sha: impl Into<String>, activities: Vec<ActivityPreview>) -> Self {
        Self {
            commit_sha: commit_sha.into(),
            activities,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentRunStatus {
    Planned,
    Running,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssessmentProgress {
    pub completed: usize,
    pub total: usize,
    pub latest_result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssessmentRun {
    pub preview: AssessmentPreview,
    pub status: AssessmentRunStatus,
    pub progress: AssessmentProgress,
    pub partial_results: bool,
    pub status_detail: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("assessment run cannot {action} while {status:?}")]
pub struct AssessmentRunError {
    action: &'static str,
    status: AssessmentRunStatus,
}

impl AssessmentRun {
    pub fn new(preview: AssessmentPreview) -> Self {
        let total = preview.activities.len();
        Self {
            preview,
            status: AssessmentRunStatus::Planned,
            progress: AssessmentProgress {
                completed: 0,
                total,
                latest_result: None,
            },
            partial_results: false,
            status_detail: None,
        }
    }

    pub fn start(&mut self) -> Result<(), AssessmentRunError> {
        if self.status != AssessmentRunStatus::Planned {
            return Err(self.error("start"));
        }
        self.status = if self.progress.total == 0 {
            AssessmentRunStatus::Completed
        } else {
            AssessmentRunStatus::Running
        };
        Ok(())
    }

    pub fn complete_activity(
        &mut self,
        result: impl Into<String>,
    ) -> Result<(), AssessmentRunError> {
        if self.status != AssessmentRunStatus::Running {
            return Err(self.error("record progress"));
        }
        self.progress.completed += 1;
        self.progress.latest_result = Some(result.into());
        if self.progress.completed == self.progress.total {
            self.status = AssessmentRunStatus::Completed;
        }
        Ok(())
    }

    pub fn cancel(&mut self, detail: impl Into<String>) -> Result<(), AssessmentRunError> {
        if !matches!(
            self.status,
            AssessmentRunStatus::Planned | AssessmentRunStatus::Running
        ) {
            return Err(self.error("cancel"));
        }
        self.status = AssessmentRunStatus::Cancelled;
        self.partial_results = self.progress.completed > 0;
        self.status_detail = Some(detail.into());
        Ok(())
    }

    pub fn results_label(&self) -> &'static str {
        if self.partial_results {
            "Partial results"
        } else if self.status == AssessmentRunStatus::Completed {
            "Complete results"
        } else {
            "No results"
        }
    }

    fn error(&self, action: &'static str) -> AssessmentRunError {
        AssessmentRunError {
            action,
            status: self.status,
        }
    }
}
