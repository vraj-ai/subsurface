# The Site Report is the improvement workspace, while the Site stays untouched

The Site Report now turns Findings into Opportunities, may use OpenCode to
prepare a candidate in a disposable clone, runs only approved checks there, and
publishes approved Work Items to an external tracker. This supersedes ADR-0008's
read-only report because a report that cannot carry an improvement through
preparation and verification does not improve the codebase; the safety boundary
moves to never applying a candidate to the active Site.
