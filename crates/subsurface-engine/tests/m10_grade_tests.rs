use std::collections::BTreeMap;
use std::path::Path;

use subsurface_engine::grade::{
    CorrectnessMetrics, Grade, GradeOverride, HardFailure, LetterGrade, MaintainabilityMetrics,
    Measurement, ProvisionalAssessment, QualityDimension, QualityMeasurements, SecurityFindings,
};
use subsurface_engine::store::SqliteStore;

#[test]
fn lowest_dimension_and_incomplete_rules() {
    let grade = excellent_measurements().with_coverage(82.9).grade(None);

    assert_eq!(grade.overall, Grade::Letter(LetterGrade::B));
    assert_eq!(grade.overall_score, Some(85));
    assert_eq!(grade.dimensions.len(), 6);
    assert!(grade.missing_dimensions.is_empty());

    let incomplete = QualityMeasurements {
        maintainability: None,
        ..excellent_measurements()
    }
    .grade(None);
    assert_eq!(incomplete.overall, Grade::Incomplete);
    assert_eq!(incomplete.overall_score, None);
    assert_eq!(
        incomplete.missing_dimensions,
        vec![QualityDimension::Maintainability]
    );
    assert!(!incomplete.automation_eligible);
}

#[test]
fn metric_bands_and_hard_failure_caps_are_strict() {
    for (score, expected) in [
        (95, LetterGrade::APlus),
        (94, LetterGrade::A),
        (89, LetterGrade::B),
        (79, LetterGrade::C),
        (69, LetterGrade::D),
        (59, LetterGrade::F),
    ] {
        assert_eq!(LetterGrade::from_score(score), expected);
    }

    for (coverage, expected) in [
        (95.0, LetterGrade::APlus),
        (94.9, LetterGrade::A),
        (90.0, LetterGrade::A),
        (89.9, LetterGrade::B),
        (80.0, LetterGrade::B),
        (79.9, LetterGrade::C),
        (70.0, LetterGrade::C),
        (69.9, LetterGrade::D),
        (50.0, LetterGrade::D),
        (49.9, LetterGrade::F),
    ] {
        assert_dimension(
            excellent_measurements().with_coverage(coverage).grade(None),
            QualityDimension::TestProtection,
            expected,
        );
    }
    for (index, expected) in [
        (85.0, LetterGrade::APlus),
        (84.9, LetterGrade::A),
        (70.0, LetterGrade::A),
        (69.9, LetterGrade::B),
        (50.0, LetterGrade::B),
        (49.9, LetterGrade::C),
        (20.0, LetterGrade::C),
        (19.9, LetterGrade::D),
        (10.0, LetterGrade::D),
        (9.9, LetterGrade::F),
    ] {
        assert_dimension(
            excellent_measurements()
                .with_maintainability(index, 4)
                .grade(None),
            QualityDimension::Maintainability,
            expected,
        );
    }
    for (complexity, expected) in [
        (5, LetterGrade::APlus),
        (6, LetterGrade::A),
        (10, LetterGrade::A),
        (11, LetterGrade::B),
        (15, LetterGrade::B),
        (16, LetterGrade::C),
        (20, LetterGrade::C),
        (21, LetterGrade::D),
        (25, LetterGrade::D),
        (26, LetterGrade::F),
    ] {
        assert_dimension(
            excellent_measurements()
                .with_maintainability(98.0, complexity)
                .grade(None),
            QualityDimension::Maintainability,
            expected,
        );
    }

    let build_failure = QualityMeasurements {
        correctness: Some(Measurement::new(
            CorrectnessMetrics {
                build_passed: false,
                locked_tests_passed: false,
            },
            "cargo build exited 1",
        )),
        test_protection: None,
        ..excellent_measurements()
    }
    .grade(None);
    assert_eq!(build_failure.overall, Grade::Letter(LetterGrade::F));
    assert!(build_failure.hard_failures.contains(&HardFailure::Build));
    assert!(build_failure
        .hard_failures
        .contains(&HardFailure::LockedTests));
    assert!(build_failure
        .missing_dimensions
        .contains(&QualityDimension::TestProtection));

    let critical_security = excellent_measurements()
        .with_security(SecurityFindings {
            blocker: 0,
            critical: 1,
            high: 0,
            medium: 0,
            low: 0,
        })
        .grade(None);
    assert_eq!(critical_security.overall, Grade::Letter(LetterGrade::F));
    assert!(critical_security
        .hard_failures
        .contains(&HardFailure::CriticalSecurity));

    for (findings, expected) in [
        (
            SecurityFindings {
                blocker: 0,
                critical: 0,
                high: 1,
                medium: 0,
                low: 0,
            },
            LetterGrade::D,
        ),
        (
            SecurityFindings {
                blocker: 0,
                critical: 0,
                high: 0,
                medium: 1,
                low: 0,
            },
            LetterGrade::C,
        ),
        (
            SecurityFindings {
                blocker: 0,
                critical: 0,
                high: 0,
                medium: 0,
                low: 1,
            },
            LetterGrade::B,
        ),
    ] {
        assert_dimension(
            excellent_measurements().with_security(findings).grade(None),
            QualityDimension::Security,
            expected,
        );
    }
}

#[test]
fn override_always_disclosed_in_receipt() {
    let project = Path::new("/tmp/strict-project");
    let store = SqliteStore::in_memory().unwrap();
    let overrides = GradeOverride {
        score_caps: BTreeMap::from([(QualityDimension::Maintainability, 70)]),
        rationale: "Legacy code must meet the stricter migration gate".into(),
    };

    store.save_grade_override(project, &overrides).unwrap();
    let persisted = store.load_grade_override(project).unwrap().unwrap();
    let grade = excellent_measurements().grade(Some(&persisted));

    assert_eq!(grade.overall, Grade::Letter(LetterGrade::C));
    assert_eq!(grade.override_receipts.len(), 1);
    assert_eq!(
        grade.override_receipts[0].dimension,
        QualityDimension::Maintainability
    );
    assert_eq!(grade.override_receipts[0].measured_score, Some(100));
    assert_eq!(grade.override_receipts[0].effective_score, Some(70));
    assert!(grade.override_receipts[0]
        .rationale
        .contains("stricter migration gate"));
}

#[test]
fn provisional_cannot_override_incomplete_or_hard_fail() {
    let claims = ProvisionalAssessment {
        critique: "The model believes every dimension is excellent".into(),
        citations: vec!["src/lib.rs:1".into()],
        suggested_scores: BTreeMap::from([
            (QualityDimension::TestProtection, 100),
            (QualityDimension::Correctness, 100),
        ]),
    };
    let incomplete = QualityMeasurements {
        test_protection: None,
        ..excellent_measurements()
    }
    .grade(None)
    .with_provisional(claims.clone());
    assert_eq!(incomplete.overall, Grade::Incomplete);
    assert!(incomplete
        .missing_dimensions
        .contains(&QualityDimension::TestProtection));
    assert!(!incomplete.automation_eligible);

    let hard_fail = QualityMeasurements {
        correctness: Some(Measurement::new(
            CorrectnessMetrics {
                build_passed: false,
                locked_tests_passed: true,
            },
            "cargo build exited 1",
        )),
        ..excellent_measurements()
    }
    .grade(None)
    .with_provisional(claims);
    assert_eq!(hard_fail.overall, Grade::Letter(LetterGrade::F));
    assert!(hard_fail.hard_failures.contains(&HardFailure::Build));
    assert!(!hard_fail.automation_eligible);
}

fn excellent_measurements() -> QualityMeasurements {
    QualityMeasurements {
        correctness: Some(Measurement::new(
            CorrectnessMetrics {
                build_passed: true,
                locked_tests_passed: true,
            },
            "cargo build and locked tests passed",
        )),
        test_protection: Some(Measurement::new(98.0, "changed-code coverage 98%")),
        security: Some(Measurement::new(
            SecurityFindings {
                blocker: 0,
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
            },
            "security scan: no findings",
        )),
        maintainability: Some(Measurement::new(
            MaintainabilityMetrics {
                maintainability_index: 98.0,
                max_changed_function_complexity: 4,
            },
            "maintainability index 98; changed-function complexity 4",
        )),
        simplicity: Some(Measurement::new(98, "simplicity checks score 98")),
        evidence_fit: Some(Measurement::new(98, "Evidence fit receipt score 98")),
    }
}

fn assert_dimension(
    grade: subsurface_engine::grade::QualityGrade,
    dimension: QualityDimension,
    expected: LetterGrade,
) {
    assert_eq!(
        grade
            .dimensions
            .iter()
            .find(|item| item.dimension == dimension)
            .unwrap()
            .grade,
        expected
    );
}

trait MeasurementFixture {
    fn with_coverage(self, value: f64) -> Self;
    fn with_maintainability(self, index: f64, complexity: u32) -> Self;
    fn with_security(self, findings: SecurityFindings) -> Self;
}

impl MeasurementFixture for QualityMeasurements {
    fn with_coverage(mut self, value: f64) -> Self {
        self.test_protection = Some(Measurement::new(value, "changed-code coverage"));
        self
    }

    fn with_maintainability(mut self, index: f64, complexity: u32) -> Self {
        self.maintainability = Some(Measurement::new(
            MaintainabilityMetrics {
                maintainability_index: index,
                max_changed_function_complexity: complexity,
            },
            "maintainability and complexity report",
        ));
        self
    }

    fn with_security(mut self, findings: SecurityFindings) -> Self {
        self.security = Some(Measurement::new(findings, "security report"));
        self
    }
}
