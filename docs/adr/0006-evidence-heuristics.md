# Related tests are found by co-commit, and staleness needs a receipt

Two places where v1 could cheat, and how it doesn't.

**Which tests protect this code.** A test file changed in the same commit as the
code is treated as related Evidence. This is pure git — no build, no language
parsing, works on any repo — and it is sometimes wrong, so it surfaces as
weaker Confidence rather than as fact. The correct alternative, running the suite
under coverage, requires Subsurface to build and execute arbitrary projects, and
fails on most repositories it would be pointed at.

**Whether a workaround is dead.** Subsurface flags staleness only when Evidence
says so: the linked issue is closed, or the dependency the code worked around has
since been upgraded past the version that needed it. No receipt, no flag. Age,
churn, and TODO comments are not evidence — flagging on those is the
confabulation `docs/adr/0001` exists to prevent, and the cost of being wrong is
someone deleting a workaround that still matters.
