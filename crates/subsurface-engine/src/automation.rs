use std::collections::BTreeSet;
use std::fmt;

use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::github::{
    publish_work_item, GitHubAuth, GitHubClient, GitHubDestination, GitHubError, PublishedWorkItem,
};
use crate::opportunity::{Opportunity, OpportunityStatus};

/// Strict letter grade used as the auto-publish minimum and candidate gate.
/// `Incomplete` is not a letter and never meets a threshold.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum QualityGrade {
    APlus,
    A,
    B,
    C,
    D,
    F,
    Incomplete,
}

impl QualityGrade {
    /// Rank from worst (0) to best (5). `Incomplete` has no rank.
    fn rank(self) -> Option<u8> {
        match self {
            Self::F => Some(0),
            Self::D => Some(1),
            Self::C => Some(2),
            Self::B => Some(3),
            Self::A => Some(4),
            Self::APlus => Some(5),
            Self::Incomplete => None,
        }
    }

    /// Selecting C includes C, B, A, and A+. `Incomplete` never qualifies.
    pub fn meets_minimum(self, minimum: Self) -> bool {
        match (self.rank(), minimum.rank()) {
            (Some(grade), Some(floor)) => grade >= floor,
            _ => false,
        }
    }
}

impl fmt::Display for QualityGrade {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::APlus => "A+",
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::F => "F",
            Self::Incomplete => "Incomplete",
        })
    }
}

/// Why automatic publication did not proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoPublishBlock {
    Disabled,
    CategoryNotEnabled {
        category: String,
    },
    NoProvenExample {
        category: String,
    },
    Incomplete,
    ModelOnly,
    GradeBelowMinimum {
        grade: QualityGrade,
        minimum: QualityGrade,
    },
    DailyLimitReached {
        limit: u32,
    },
    NotVerified {
        status: OpportunityStatus,
    },
}

impl fmt::Display for AutoPublishBlock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => formatter.write_str("auto-publish is off"),
            Self::CategoryNotEnabled { category } => {
                write!(formatter, "category {category} is not enabled")
            }
            Self::NoProvenExample { category } => {
                write!(
                    formatter,
                    "category {category} has no proven manual example"
                )
            }
            Self::Incomplete => formatter.write_str("Incomplete is never eligible"),
            Self::ModelOnly => formatter.write_str("model-only assessments are never eligible"),
            Self::GradeBelowMinimum { grade, minimum } => {
                write!(formatter, "grade {grade} is below minimum {minimum}")
            }
            Self::DailyLimitReached { limit } => {
                write!(formatter, "daily publication limit {limit} reached")
            }
            Self::NotVerified { status } => {
                write!(formatter, "Opportunity is {status:?}, not Verified")
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum AutoPublishError {
    #[error("no auto-publication countdown is pending")]
    NoPendingCountdown,
    #[error("countdown is for {pending}, not {requested}")]
    CountdownMismatch { pending: String, requested: String },
    #[error("{0}")]
    Blocked(AutoPublishBlock),
    #[error(transparent)]
    GitHub(#[from] GitHubError),
}

/// Per-Project auto-publish settings. Off until explicitly enabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoPublishSettings {
    enabled: bool,
    enabled_categories: BTreeSet<String>,
    proven_categories: BTreeSet<String>,
    min_grade: QualityGrade,
    daily_limit: u32,
}

impl Default for AutoPublishSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            enabled_categories: BTreeSet::new(),
            proven_categories: BTreeSet::new(),
            min_grade: QualityGrade::APlus,
            daily_limit: 1,
        }
    }
}

impl AutoPublishSettings {
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn min_grade(&self) -> QualityGrade {
        self.min_grade
    }

    pub fn daily_limit(&self) -> u32 {
        self.daily_limit
    }

    pub fn category_enabled(&self, category: &str) -> bool {
        self.enabled_categories.contains(category)
    }

    pub fn has_proven_example(&self, category: &str) -> bool {
        self.proven_categories.contains(category)
    }

    pub fn set_min_grade(&mut self, grade: QualityGrade) {
        self.min_grade = grade;
    }

    pub fn set_daily_limit(&mut self, limit: u32) {
        self.daily_limit = limit;
    }

    pub fn enable_category(&mut self, category: impl Into<String>) {
        self.enabled_categories.insert(category.into());
    }

    /// A manually approved Work Item proves the category template.
    pub fn record_proven_example(&mut self, category: impl Into<String>) {
        self.proven_categories.insert(category.into());
    }
}

/// Measured candidate presented to auto-publish. Model-only is never eligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoPublishCandidate<'a> {
    pub opportunity: &'a Opportunity,
    pub destination: &'a GitHubDestination,
    pub grade: QualityGrade,
    pub model_only: bool,
}

/// Visible countdown shown before an automatic GitHub write.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoPublishCountdown {
    pub opportunity_id: String,
    pub category: String,
    pub destination: GitHubDestination,
    pub grade: QualityGrade,
    model_only: bool,
}

/// Auditable record of every auto-publish decision and external write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoPublishEvent {
    Blocked {
        opportunity_id: String,
        category: String,
        reason: AutoPublishBlock,
    },
    CountdownStarted {
        opportunity_id: String,
        category: String,
        destination: GitHubDestination,
        grade: QualityGrade,
    },
    Cancelled {
        opportunity_id: String,
        category: String,
    },
    Published {
        opportunity_id: String,
        category: String,
        number: u64,
        html_url: String,
        fingerprint: String,
    },
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoPublishDecision {
    Blocked(AutoPublishBlock),
    Countdown(AutoPublishCountdown),
}

/// Guarded opt-in auto-publisher. Off by default; GitHub writes go through the
/// same fingerprinted publish path as manual Work Items.
pub struct AutoPublisher {
    settings: AutoPublishSettings,
    log: Vec<AutoPublishEvent>,
    pending: Option<AutoPublishCountdown>,
    day: NaiveDate,
    published_today: u32,
}

impl Default for AutoPublisher {
    fn default() -> Self {
        Self::new(AutoPublishSettings::default())
    }
}

impl AutoPublisher {
    pub fn new(settings: AutoPublishSettings) -> Self {
        Self {
            settings,
            log: Vec::new(),
            pending: None,
            day: Utc::now().date_naive(),
            published_today: 0,
        }
    }

    pub fn settings(&self) -> &AutoPublishSettings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut AutoPublishSettings {
        &mut self.settings
    }

    pub fn log(&self) -> &[AutoPublishEvent] {
        &self.log
    }

    pub fn pending(&self) -> Option<&AutoPublishCountdown> {
        self.pending.as_ref()
    }

    pub fn published_today(&self) -> u32 {
        self.published_today
    }

    /// Turn auto-publish on. Still requires category, proven example, grade, and limit.
    pub fn enable(&mut self) {
        self.settings.enabled = true;
    }

    /// Stop future writes immediately. Pending countdown is cancelled. Log stays.
    pub fn disable(&mut self) {
        self.settings.enabled = false;
        self.pending = None;
        self.log.push(AutoPublishEvent::Disabled);
    }

    /// Advance to the next publication day without sleeping.
    pub fn start_new_day(&mut self) {
        self.day = self.day.succ_opt().unwrap_or(self.day);
        self.published_today = 0;
    }

    pub fn consider(&mut self, candidate: &AutoPublishCandidate<'_>) -> AutoPublishDecision {
        let category = candidate.opportunity.draft.category.clone();
        let opportunity_id = candidate.opportunity.id.clone();
        if let Some(block) = self.block_reason(
            candidate.opportunity.status,
            &category,
            candidate.grade,
            candidate.model_only,
        ) {
            self.log.push(AutoPublishEvent::Blocked {
                opportunity_id,
                category,
                reason: block.clone(),
            });
            return AutoPublishDecision::Blocked(block);
        }

        let countdown = AutoPublishCountdown {
            opportunity_id: opportunity_id.clone(),
            category: category.clone(),
            destination: candidate.destination.clone(),
            grade: candidate.grade,
            model_only: candidate.model_only,
        };
        self.pending = Some(countdown.clone());
        self.log.push(AutoPublishEvent::CountdownStarted {
            opportunity_id,
            category,
            destination: countdown.destination.clone(),
            grade: countdown.grade,
        });
        AutoPublishDecision::Countdown(countdown)
    }

    pub fn cancel(&mut self) -> Option<AutoPublishCountdown> {
        let pending = self.pending.take()?;
        self.log.push(AutoPublishEvent::Cancelled {
            opportunity_id: pending.opportunity_id.clone(),
            category: pending.category.clone(),
        });
        Some(pending)
    }

    /// Complete a due countdown. Re-checks guards so disable / limit / grade
    /// changes after the countdown started still block the write.
    pub fn publish_due(
        &mut self,
        client: &GitHubClient,
        auth: &GitHubAuth,
        opportunity: &mut Opportunity,
    ) -> Result<PublishedWorkItem, AutoPublishError> {
        let pending = self
            .pending
            .take()
            .ok_or(AutoPublishError::NoPendingCountdown)?;
        if pending.opportunity_id != opportunity.id {
            self.pending = Some(pending.clone());
            return Err(AutoPublishError::CountdownMismatch {
                pending: pending.opportunity_id,
                requested: opportunity.id.clone(),
            });
        }
        if let Some(block) = self.block_reason(
            opportunity.status,
            &opportunity.draft.category,
            pending.grade,
            pending.model_only,
        ) {
            self.log.push(AutoPublishEvent::Blocked {
                opportunity_id: opportunity.id.clone(),
                category: opportunity.draft.category.clone(),
                reason: block.clone(),
            });
            return Err(AutoPublishError::Blocked(block));
        }

        let published = publish_work_item(client, &pending.destination, auth, &opportunity.draft)?;
        opportunity.record_publication(published.clone());
        self.published_today = self.published_today.saturating_add(1);
        self.log.push(AutoPublishEvent::Published {
            opportunity_id: opportunity.id.clone(),
            category: opportunity.draft.category.clone(),
            number: published.number,
            html_url: published.html_url.clone(),
            fingerprint: published.fingerprint.clone(),
        });
        Ok(published)
    }

    fn block_reason(
        &self,
        status: OpportunityStatus,
        category: &str,
        grade: QualityGrade,
        model_only: bool,
    ) -> Option<AutoPublishBlock> {
        if !self.settings.enabled {
            return Some(AutoPublishBlock::Disabled);
        }
        if status != OpportunityStatus::Verified {
            return Some(AutoPublishBlock::NotVerified { status });
        }
        if !self.settings.category_enabled(category) {
            return Some(AutoPublishBlock::CategoryNotEnabled {
                category: category.to_owned(),
            });
        }
        if !self.settings.has_proven_example(category) {
            return Some(AutoPublishBlock::NoProvenExample {
                category: category.to_owned(),
            });
        }
        if grade == QualityGrade::Incomplete {
            return Some(AutoPublishBlock::Incomplete);
        }
        if model_only {
            return Some(AutoPublishBlock::ModelOnly);
        }
        if !grade.meets_minimum(self.settings.min_grade) {
            return Some(AutoPublishBlock::GradeBelowMinimum {
                grade,
                minimum: self.settings.min_grade,
            });
        }
        if self.published_today >= self.settings.daily_limit {
            return Some(AutoPublishBlock::DailyLimitReached {
                limit: self.settings.daily_limit,
            });
        }
        None
    }
}
