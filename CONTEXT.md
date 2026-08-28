# Subsurface

A local-first desktop tool for understanding why code exists, by reading the
evidence a repository already contains.

## Language

**Site**:
An opened repository, plus the index Subsurface has built over it.

**Excavate**:
The action of investigating a selected region of code to reconstruct why it exists.

**Evidence**:
A concrete, citable record found in the Site — a commit, diff, test, or doc.
Never an inference.
_Avoid_: Source, reference

**Finding**:
Subsurface's answer for an Excavate — a claim about why code exists, each part
tied to Evidence. A Finding with no Evidence states that no rationale was
recorded; it never guesses.
_Avoid_: Explanation, summary, insight

**Confidence**:
How well the Evidence supports a Finding. May be zero.

**Quality Grade**:
A strict `A+`–`F` or `Incomplete` assessment of a Site or prepared candidate.
The overall grade is its lowest measured quality dimension and each deduction
must have a receipt.
_Avoid_: Confidence, quality score

**Site Report**:
A repo-wide improvement workspace for a Site. It grades measured quality,
surfaces Opportunities, and links each one to its Findings and Evidence.

**Opportunity**:
An evidence-backed candidate for improving a Site, derived from one or more
Findings and not yet approved for publication or implementation.
_Avoid_: Issue, suggestion

**Work Item**:
An approved Opportunity published to an external issue tracker.
_Avoid_: Opportunity, Finding

**Improvement Receipt**:
A baseline-to-candidate comparison showing what a prepared change improved,
which checks prove it, and which risks or failures remain.
_Avoid_: Quality score, test result

**Field Note**:
A saved Finding, kept so it can be revisited or shared.

**Timeline**:
The ordered history of changes to a symbol or region within a Site.
