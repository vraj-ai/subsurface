# Architecture Context

## Purpose

Subsurface is a local-first desktop tool that answers one question about a
selected region of code: **is this safe to change?** It reads the Evidence the
repository already contains, turns verified problems into Opportunities, and
can prepare and test a candidate in a disposable clone before publishing an
approved Work Item. It improves codebases without applying changes to the Project.

## Locked Decisions

- A Finding splits in two. **What/when** — what the code does, when it appeared,
  how it evolved, which tests cover it — is derived from git and always shown.
  **Why** is asserted only when Evidence supports it. See `docs/adr/0001`.
- Inference is cloud-first via a signed-in provider; Ollama is a switch. Index,
  blame, renames and Timelines never leave the machine. See `docs/adr/0002`.
- Auth ships both paths in v1: pasted keys for native providers and compatible
  endpoints (OpenAI, xAI, OpenRouter, OpenCode Zen/Go/Free, Ollama, and Custom)
  plus OAuth where the provider offers it. OpenCode Go uses its API key, not
  OAuth. No curated model list. See `docs/adr/0005`.
- External inference uses one outbound trust boundary. The preview and request
  share the same provider payload builder; consent defaults to each request,
  may be remembered only for the active Project, and offline mode blocks the
  request before a socket opens. Provider and OAuth clients do not follow
  redirects that could forward credentials or source payloads.
- Agents reach Subsurface over MCP and share its Field Notes. See `docs/adr/0004`.
- A selection is a line range, followed through history with `git log -L`.
  Symbol-level tracking is a later layer.
- Opening a Project indexes only the cheap things — commit graph, paths, renames.
  Expensive walks happen per Excavate.
- The shell is Tauri with a Rust backend. See `docs/adr/0003`.
- The primary flow is open Project -> Assessment -> Opportunity -> Finding ->
  prepare -> verify -> Work Item. Direct Excavate remains available.
- The Project Assessment is the primary improvement workspace. It grades only measured
  quality, surfaces Opportunities, and drills into the same Findings produced
  by Excavate. See `docs/adr/0009`.
- An Opportunity moves through Detected, Prepared, Verified or Failed, then
  Published or Dismissed. Preparation and tests run only in a disposable clone;
  the Project is never modified. See `docs/adr/0009`.
- A Project Assessment is bound to the opened Project's exact HEAD and persisted
  once per Project and commit. Its history retains the strict grade, baseline and
  per-dimension deltas, and Opportunities linked to the same Findings produced by
  the existing report/Excavate seam. Opportunity ordering exposes named Impact,
  verification, expected grade improvement, provisional effort, and age fields;
  it has no composite priority score.
- A Quality Grade is `A+` through `F`, or `Incomplete`, across correctness,
  test protection, security, maintainability, simplicity, and Evidence fit.
  The overall grade is the lowest measured dimension; known build, locked-test,
  or critical-security failure forces `F` while retaining any measurement-gap
  disclosure. A model may add a provisional critique but never manufacture a
  missing metric. See `docs/adr/0010`.
- Automatic Work Item publication is off by default and requires explicit
  per-Project, per-category enablement plus a user-selected minimum Quality Grade.
  `Incomplete` and model-only assessments are never eligible.
- Related tests are found by co-commit; staleness is flagged only with a
  receipt. See `docs/adr/0006`.
- Field Notes live in a per-machine SQLite database in app data, keyed by Project.
  Nothing is written into the user's repository.
- Evidence sources in v1 are git, tests, and docs. GitHub/GitLab PRs and issues
  are the next layer, not the first.
- Target hardware is 32GB, which only binds the optional local path.

## Invariants

- A Finding never asserts a rationale it cannot cite. See `docs/adr/0001`.
- Confidence is assigned by rule, is one of Stated / Inferred / None, and is
  never the model's self-rating. See `docs/adr/0007`.
- Indexing, blame, rename-following and Timelines run locally regardless of
  which provider is configured.
- Subsurface never writes into the user's active Project repository.
- Generated candidates and repository commands run only in a disposable clone
  under an explicit per-Project command allowlist.
- Every Quality Grade deduction cites an Improvement Receipt; missing required
  measurements produce `Incomplete`.
- Nothing is sent to a provider without the user seeing what it is.
- Evidence sent per Excavate is a fixed budget, ranked by how much of the
  selection each commit touched. What was left out is shown, never silently
  dropped.

## Non-goals

- Subsurface never applies a candidate to the active Project and does not replace
  an editor, git client, pull-request workflow, or documentation system.
- A prepared candidate is a verified proposal attached to a Work Item, not a
  silent edit or an automatically merged change.

## Accepted Boundaries

- v1 ships on macOS only. Tauri makes the other platforms cheap later; each is
  its own signing and webview story.
- `Project` is the canonical product and wire vocabulary. Legacy `Site` types,
  commands, and `site_path` inputs remain only during the expand/contract
  migration and must not appear in new responses or active UI copy.
- The MCP server is in-process and runs only while the app is open. Agents get
  an explicit "not running" error rather than a headless daemon.
- The co-commit test heuristic is sometimes wrong; it is surfaced as weaker
  Confidence, not as fact. See `docs/adr/0006`.
- OAuth is bought for sign-in experience only, and can slip to the key path
  without losing provider coverage. See `docs/adr/0005`.

## Ownership

- `AGENTS.md` is the always-loaded project router.
- This file is human-owned and changes only when intent, decisions, invariants,
  non-goals, or accepted boundaries change.
- `CONTEXT/progress.md` is a bounded derived pointer, not a resume source.
- `goals` owns goal backlogs, handoffs, progress updates, and review verdicts.
