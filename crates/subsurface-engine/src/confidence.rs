use serde::{Deserialize, Serialize};
use crate::evidence::{Evidence, EvidenceKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    Stated,
    Inferred,
    None,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Confidence::Stated => write!(f, "Stated"),
            Confidence::Inferred => write!(f, "Inferred"),
            Confidence::None => write!(f, "None"),
        }
    }
}

/// Assigns Confidence by rule based strictly on the shape of Evidence.
/// Pure logic, no IO, no provider.
pub fn assign_confidence(evidence: &[Evidence]) -> Confidence {
    if evidence.is_empty() {
        return Confidence::None;
    }

    // 1. Stated: A commit message or doc explicitly states the rationale in words.
    let has_stated_rationale = evidence.iter().any(|e| {
        let msg = e.message.trim().to_lowercase();
        if !is_trivial_commit_message(&msg) && has_rationale_signals(&msg) {
            return true;
        }
        false
    });

    if has_stated_rationale {
        return Confidence::Stated;
    }

    // 2. Inferred: Contextual evidence exists (e.g. co-committed tests/docs or structured change)
    let has_inferred_context = evidence.iter().any(|e| {
        e.kind == EvidenceKind::CoCommittedTest
            || e.kind == EvidenceKind::CoCommittedDoc
            || !is_trivial_commit_message(&e.message.trim().to_lowercase())
    });

    if has_inferred_context {
        return Confidence::Inferred;
    }

    // 3. None: No rationale recorded
    Confidence::None
}

fn is_trivial_commit_message(msg: &str) -> bool {
    let lower = msg.trim().to_lowercase();
    let stripped = lower.trim_matches(|c: char| !c.is_alphanumeric());
    matches!(
        stripped,
        "" | "fix"
            | "wip"
            | "work in progress"
            | "patch"
            | "update"
            | "temp"
            | "changes"
            | "clean up"
            | "cleanup"
            | "fix typo"
            | "misc"
            | "formatting"
            | "test"
            | "wip..."
    )
}

fn has_rationale_signals(msg: &str) -> bool {
    let keywords = [
        "because",
        "due to",
        "workaround",
        "in order to",
        "fix for #",
        "fixes #",
        "closes #",
        "needed for",
        "prevent",
        "avoids",
        "required by",
        "reason:",
        "rationale:",
        "issue #",
    ];

    for kw in &keywords {
        if msg.contains(kw) {
            return true;
        }
    }

    if msg.len() >= 50 && (msg.contains(':') || msg.contains('.') || msg.contains('\n')) {
        return true;
    }

    false
}
