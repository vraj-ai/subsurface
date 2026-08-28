use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum QualityDimension {
    Correctness,
    TestProtection,
    Security,
    Maintainability,
    Simplicity,
    EvidenceFit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum LetterGrade {
    F,
    D,
    C,
    B,
    A,
    #[serde(rename = "A+")]
    APlus,
}

impl LetterGrade {
    pub fn from_score(score: u8) -> Self {
        match score {
            95..=100 => Self::APlus,
            90..=94 => Self::A,
            80..=89 => Self::B,
            70..=79 => Self::C,
            60..=69 => Self::D,
            _ => Self::F,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Grade {
    Incomplete,
    Letter(LetterGrade),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Measurement<T> {
    pub value: T,
    pub receipt: String,
}

impl<T> Measurement<T> {
    pub fn new(value: T, receipt: impl Into<String>) -> Self {
        Self {
            value,
            receipt: receipt.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorrectnessMetrics {
    pub build_passed: bool,
    pub locked_tests_passed: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MaintainabilityMetrics {
    pub maintainability_index: f64,
    pub max_changed_function_complexity: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityFindings {
    pub blocker: u32,
    pub critical: u32,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityMeasurements {
    pub correctness: Option<Measurement<CorrectnessMetrics>>,
    pub test_protection: Option<Measurement<f64>>,
    pub security: Option<Measurement<SecurityFindings>>,
    pub maintainability: Option<Measurement<MaintainabilityMetrics>>,
    pub simplicity: Option<Measurement<u8>>,
    pub evidence_fit: Option<Measurement<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GradeOverride {
    pub score_caps: BTreeMap<QualityDimension, u8>,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverrideReceipt {
    pub dimension: QualityDimension,
    pub measured_score: Option<u8>,
    pub effective_score: Option<u8>,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DimensionGrade {
    pub dimension: QualityDimension,
    pub score: u8,
    pub grade: LetterGrade,
    pub receipt: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HardFailure {
    Build,
    LockedTests,
    CriticalSecurity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvisionalAssessment {
    pub critique: String,
    pub citations: Vec<String>,
    pub suggested_scores: BTreeMap<QualityDimension, u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QualityGrade {
    pub overall: Grade,
    pub overall_score: Option<u8>,
    pub dimensions: Vec<DimensionGrade>,
    pub missing_dimensions: Vec<QualityDimension>,
    pub hard_failures: Vec<HardFailure>,
    pub override_receipts: Vec<OverrideReceipt>,
    pub provisional: Option<ProvisionalAssessment>,
    pub automation_eligible: bool,
}

impl QualityGrade {
    pub fn with_provisional(mut self, provisional: ProvisionalAssessment) -> Self {
        self.provisional = Some(provisional);
        self.automation_eligible = false;
        self
    }
}

impl QualityMeasurements {
    pub fn grade(&self, overrides: Option<&GradeOverride>) -> QualityGrade {
        let mut dimensions = Vec::with_capacity(6);
        let mut missing = Vec::new();
        let mut hard_failures = Vec::new();

        push_measurement(
            &mut dimensions,
            &mut missing,
            QualityDimension::Correctness,
            self.correctness.as_ref(),
            |metrics| {
                if metrics.build_passed && metrics.locked_tests_passed {
                    100
                } else {
                    0
                }
            },
        );
        if let Some(measurement) = &self.correctness {
            if !measurement.value.build_passed {
                hard_failures.push(HardFailure::Build);
            }
            if !measurement.value.locked_tests_passed {
                hard_failures.push(HardFailure::LockedTests);
            }
        }

        push_float_measurement(
            &mut dimensions,
            &mut missing,
            QualityDimension::TestProtection,
            self.test_protection.as_ref(),
            coverage_score,
        );
        push_measurement(
            &mut dimensions,
            &mut missing,
            QualityDimension::Security,
            self.security.as_ref(),
            security_score,
        );
        if self
            .security
            .as_ref()
            .is_some_and(|item| item.value.blocker > 0 || item.value.critical > 0)
        {
            hard_failures.push(HardFailure::CriticalSecurity);
        }
        push_measurement(
            &mut dimensions,
            &mut missing,
            QualityDimension::Maintainability,
            self.maintainability
                .as_ref()
                .filter(|item| item.value.maintainability_index.is_finite()),
            |metrics| {
                maintainability_score(metrics.maintainability_index)
                    .min(complexity_score(metrics.max_changed_function_complexity))
            },
        );
        push_measurement(
            &mut dimensions,
            &mut missing,
            QualityDimension::Simplicity,
            self.simplicity.as_ref(),
            |score| *score,
        );
        push_measurement(
            &mut dimensions,
            &mut missing,
            QualityDimension::EvidenceFit,
            self.evidence_fit.as_ref(),
            |score| *score,
        );

        let mut override_receipts = Vec::new();
        if let Some(overrides) = overrides {
            for (dimension, cap) in &overrides.score_caps {
                let measured = dimensions
                    .iter()
                    .find(|item| item.dimension == *dimension)
                    .map(|item| item.score);
                let effective = measured.map(|score| score.min((*cap).min(100)));
                if let Some(item) = dimensions
                    .iter_mut()
                    .find(|item| item.dimension == *dimension)
                {
                    item.score = effective.unwrap_or(item.score);
                    item.grade = LetterGrade::from_score(item.score);
                }
                override_receipts.push(OverrideReceipt {
                    dimension: *dimension,
                    measured_score: measured,
                    effective_score: effective,
                    rationale: overrides.rationale.clone(),
                });
            }
        }

        let weakest = dimensions.iter().map(|item| item.score).min().unwrap_or(0);
        let (overall, overall_score) = if !hard_failures.is_empty() {
            (Grade::Letter(LetterGrade::F), Some(0))
        } else if !missing.is_empty() {
            (Grade::Incomplete, None)
        } else {
            (
                Grade::Letter(LetterGrade::from_score(weakest)),
                Some(weakest),
            )
        };
        QualityGrade {
            overall,
            overall_score,
            dimensions,
            missing_dimensions: missing,
            automation_eligible: overall_score.is_some() && hard_failures.is_empty(),
            hard_failures,
            override_receipts,
            provisional: None,
        }
    }
}

fn coverage_score(value: f64) -> u8 {
    match value.clamp(0.0, 100.0) {
        value if value >= 95.0 => 100,
        value if value >= 90.0 => 92,
        value if value >= 80.0 => 85,
        value if value >= 70.0 => 75,
        value if value >= 50.0 => 65,
        _ => 0,
    }
}

fn maintainability_score(value: f64) -> u8 {
    match value.clamp(0.0, 100.0) {
        value if value >= 85.0 => 100,
        value if value >= 70.0 => 92,
        value if value >= 50.0 => 85,
        value if value >= 20.0 => 75,
        value if value >= 10.0 => 65,
        _ => 0,
    }
}

fn complexity_score(value: u32) -> u8 {
    match value {
        0..=5 => 100,
        6..=10 => 92,
        11..=15 => 85,
        16..=20 => 75,
        21..=25 => 65,
        _ => 0,
    }
}

fn security_score(findings: &SecurityFindings) -> u8 {
    if findings.blocker > 0 || findings.critical > 0 {
        0
    } else if findings.high > 0 {
        65
    } else if findings.medium > 0 {
        75
    } else if findings.low > 0 {
        85
    } else {
        100
    }
}

fn push_float_measurement(
    dimensions: &mut Vec<DimensionGrade>,
    missing: &mut Vec<QualityDimension>,
    dimension: QualityDimension,
    measurement: Option<&Measurement<f64>>,
    score: impl FnOnce(f64) -> u8,
) {
    let valid = measurement.filter(|item| item.value.is_finite());
    push_measurement(dimensions, missing, dimension, valid, |value| score(*value));
}

fn push_measurement<T>(
    dimensions: &mut Vec<DimensionGrade>,
    missing: &mut Vec<QualityDimension>,
    dimension: QualityDimension,
    measurement: Option<&Measurement<T>>,
    score: impl FnOnce(&T) -> u8,
) {
    let Some(measurement) = measurement.filter(|item| !item.receipt.trim().is_empty()) else {
        missing.push(dimension);
        return;
    };
    let score = score(&measurement.value).min(100);
    dimensions.push(DimensionGrade {
        dimension,
        score,
        grade: LetterGrade::from_score(score),
        receipt: measurement.receipt.clone(),
    });
}
