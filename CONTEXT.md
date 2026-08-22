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

**Site Report**:
A read-only, repo-wide view of a Site: which regions have no recorded rationale,
which look dead, and which nothing tests. The Excavate engine run across
everything instead of one selection. It names problems; it never fixes them.

**Field Note**:
A saved Finding, kept so it can be revisited or shared.

**Timeline**:
The ordered history of changes to a symbol or region within a Site.
