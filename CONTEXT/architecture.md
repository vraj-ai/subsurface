# Architecture Context

## Purpose

Subsurface is a local-first desktop tool that answers one question about a
selected region of code: **is this safe to change?** It answers by reading the
evidence the repository already contains — for one selected region on demand,
and across a whole Site in the Site Report. It exists to make codebases better
by making their history legible, without writing code itself.

## Locked Decisions

- A Finding splits in two. **What/when** — what the code does, when it appeared,
  how it evolved, which tests cover it — is derived from git and always shown.
  **Why** is asserted only when Evidence supports it. See `docs/adr/0001`.
- Inference is cloud-first via a signed-in provider; Ollama is a switch. Index,
  blame, renames and Timelines never leave the machine. See `docs/adr/0002`.
- Auth ships both paths in v1: pasted keys for any OpenAI-compatible endpoint
  (presets for OpenAI, Grok, OpenRouter, OpenCode Zen) and OAuth where the
  provider offers it. No curated model list. See `docs/adr/0005`.
- Agents reach Subsurface over MCP and share its Field Notes. See `docs/adr/0004`.
- A selection is a line range, followed through history with `git log -L`.
  Symbol-level tracking is a later layer.
- Opening a Site indexes only the cheap things — commit graph, paths, renames.
  Expensive walks happen per Excavate.
- The shell is Tauri with a Rust backend. See `docs/adr/0003`.
- The primary flow is select code -> Excavate -> Finding.
- v1 also ships the Site Report: a read-only, repo-level view of what has no
  recorded rationale, what looks dead, and what nothing tests. It is a saved
  query over the Excavate engine, not a separate one. See `docs/adr/0008`.
- Related tests are found by co-commit; staleness is flagged only with a
  receipt. See `docs/adr/0006`.
- Field Notes live in a per-machine SQLite database in app data, keyed by Site.
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
- Subsurface never writes into the user's repository.
- Nothing is sent to a provider without the user seeing what it is.
- Evidence sent per Excavate is a fixed budget, ranked by how much of the
  selection each commit touched. What was left out is shown, never silently
  dropped.

## Non-goals

- Subsurface never writes or suggests code edits. It may flag that a workaround
  looks dead and show why; the change is the developer's.
- Not an editor, a git client, a coding assistant, or a documentation generator.

## Accepted Boundaries

- v1 ships on macOS only. Tauri makes the other platforms cheap later; each is
  its own signing and webview story.
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
