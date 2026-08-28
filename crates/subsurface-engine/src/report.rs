use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::confidence::Confidence;
use crate::evidence::LineRange;
use crate::excavate::{excavate, Finding};
use crate::provider::Provider;
use crate::site::Site;
use crate::staleness::StalenessStatus;

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("Project has no HEAD commit")]
    NoHeadCommit,
    #[error("Excavate failed: {0}")]
    Excavate(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SiteReportCategory {
    DeadWorkaround,
    NoRationale,
    UntestedCode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SiteReportEntry {
    pub file_path: String,
    pub line_range: LineRange,
    pub category: SiteReportCategory,
    pub description: String,
    pub finding: Finding,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SiteReport {
    #[serde(rename = "project_path", alias = "site_path")]
    pub site_path: PathBuf,
    pub generated_at: String,
    pub head_commit: String,
    pub total_files_scanned: usize,
    pub entries: Vec<SiteReportEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteReportEstimate {
    pub file_count: usize,
    pub estimated_duration_secs: f64,
    pub estimated_token_cost: usize,
}

pub fn estimate_site_report_cost(site: &Site, filter_prefix: Option<&str>) -> SiteReportEstimate {
    let files: Vec<&String> = site
        .tracked_files
        .iter()
        .filter(|f| {
            if let Some(prefix) = filter_prefix {
                f.starts_with(prefix)
            } else {
                true
            }
        })
        .filter(|f| is_eligible_code_file(f))
        .collect();

    let count = files.len();
    let estimated_duration_secs = (count as f64) * 0.15;
    let estimated_token_cost = count * 600;

    SiteReportEstimate {
        file_count: count,
        estimated_duration_secs,
        estimated_token_cost,
    }
}

pub fn generate_site_report(
    site: &Site,
    filter_prefix: Option<&str>,
    provider: Arc<dyn Provider>,
) -> Result<SiteReport, ReportError> {
    let head_commit = site
        .head_commit
        .clone()
        .ok_or(ReportError::NoHeadCommit)?;
    let now = Utc::now().to_rfc3339();

    let eligible_files: Vec<&String> = site
        .tracked_files
        .iter()
        .filter(|f| {
            if let Some(prefix) = filter_prefix {
                f.starts_with(prefix)
            } else {
                true
            }
        })
        .filter(|f| is_eligible_code_file(f))
        .collect();

    let mut entries = Vec::new();

    for file_path in &eligible_files {
        let full_path = site.root_path.join(file_path);
        let content = fs::read_to_string(&full_path).unwrap_or_default();
        let line_count = content.lines().count();
        if line_count == 0 {
            continue;
        }

        let range = LineRange {
            start: 1,
            end: line_count.min(100),
        };

        let finding = excavate(site, file_path, range, provider.clone())
            .map_err(|e| ReportError::Excavate(e.to_string()))?;

        if let StalenessStatus::Stale { ref receipt } = finding.staleness {
            entries.push(SiteReportEntry {
                file_path: file_path.to_string(),
                line_range: range,
                category: SiteReportCategory::DeadWorkaround,
                description: format!("Dead workaround receipt: {}", receipt.receipt_text),
                finding: finding.clone(),
            });
        } else if finding.why.confidence == Confidence::None {
            entries.push(SiteReportEntry {
                file_path: file_path.to_string(),
                line_range: range,
                category: SiteReportCategory::NoRationale,
                description: "No recorded rationale found in commit history or docs.".to_string(),
                finding: finding.clone(),
            });
        } else if finding.what_when.related_tests.is_empty()
            && !file_path.contains("test")
            && !file_path.contains("doc")
        {
            entries.push(SiteReportEntry {
                file_path: file_path.to_string(),
                line_range: range,
                category: SiteReportCategory::UntestedCode,
                description: "No co-committed tests found in change history.".to_string(),
                finding: finding.clone(),
            });
        }
    }

    Ok(SiteReport {
        site_path: site.root_path.clone(),
        generated_at: now,
        head_commit,
        total_files_scanned: eligible_files.len(),
        entries,
    })
}

fn is_eligible_code_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    !lower.ends_with(".lock")
        && !lower.ends_with(".png")
        && !lower.ends_with(".jpg")
        && !lower.ends_with(".ico")
        && !lower.ends_with(".icns")
        && !lower.ends_with(".svg")
        && !lower.ends_with(".gitignore")
        && !lower.ends_with(".json")
}
