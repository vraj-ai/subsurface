# The Site Report ships in v1, and stays read-only

Status: superseded by ADR-0009

Subsurface is meant to improve codebases, not only explain them, so v1 ships a
repo-level report alongside the single Excavate: which areas have no recorded
rationale, which workarounds have a receipt saying they are dead, which regions
nothing tests. It is a saved query over the same engine — one Excavate's
machinery, run across a Site — not a second product.

It is read-only. No work queue, no assignments, no proposed diffs. Subsurface
names what looks wrong and cites why; deciding and changing is the developer's,
which keeps `CONTEXT/architecture.md`'s Non-goals intact.

The cost of pulling it forward: the report's ranking is only as good as the
Findings under it, and v1 is where we first learn what a good Finding looks
like. Expect the ranking to be wrong at first and to be re-tuned once real
Findings exist. Accepted — the report is what makes v1 useful before you know
which line to select.
