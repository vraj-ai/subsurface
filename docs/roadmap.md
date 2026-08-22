# Roadmap

Everything deliberately cut from v1, and what it waits on. Nothing here is
rejected — rejected things are Non-goals in `CONTEXT/architecture.md`.

Versions are ordered, not dated. A feature moves up when the thing it waits on
exists, not when someone feels like it.

## v1 — one Excavate, done properly

Select a line range, get a Finding: what the code does, when it appeared, how it
evolved, which tests touch it, and why — where a receipt exists. Plus the Site
Report: the same engine run across the whole repo, read-only. Git, tests and
docs as Evidence. Provider auth. MCP server. Field Notes in SQLite.

## v1.1 — the evidence that actually says why

- **GitHub / GitLab PRs and issues as Evidence.** The real rationale usually
  lives in a PR comment, not a commit message. Waits on v1 because it drags in
  network auth, rate limits, and per-forge APIs, and v1's answer quality is what
  tells us how much it's worth.
- **Staleness receipts from issue state.** Once issues are indexed, "the linked
  issue is closed" becomes checkable, which is half of `docs/adr/0006`.
- **Field Note export.** Markdown out of the SQLite store, so an investigation
  can be shared or committed by hand.

## v1.2 — the PR reviewer

- **Review bot for PRs and commits.** The CodeRabbit shape, with the thing
  CodeRabbit does not have: history. A reviewer that already knows why the code
  it is reading exists, which past commit introduced the constraint being
  broken, and which workaround is being removed with no receipt saying it is
  dead. Review comments cite Evidence under `docs/adr/0001`'s rule — no receipt,
  no claim.
- Waits on v1.1, because PR and issue ingestion is the same forge integration
  the reviewer needs, and on v1 because a review comment is a Finding with a
  different presentation.
- Provider is whatever the user configured under `docs/adr/0005`; the bot ships
  no model of its own.

## v1.3 — symbols instead of line ranges

- **Language parsers.** Track a function or class through history instead of a
  line range. One parser per language, so it lands language by language, most
  used first. Line ranges keep working everywhere else.
- **Coverage-based test linking**, if and only if the co-commit heuristic proves
  too noisy in practice. Requires building and running the user's project, which
  is why it is not the default.

## v1.4 — more than one dig at a time

- **Actions on the Site Report.** Turning a reported problem into a tracked work
  item, or handing it to an agent over MCP. Deliberately absent from v1: the
  report names problems, it does not manage them.
- **Background analysis.** Keeping the Site Report fresh without being asked.
  Waits until the report's ranking has been re-tuned against real Findings.
- **Multiple Sites in one workspace.**

## Later, unscheduled

- **Jump in from outside** — open Subsurface at an exact location from a file
  path, a pasted GitHub link, or a terminal command.
- **Orientation flow** for a new joiner reading an unfamiliar codebase, and
  **cleanup flow** for a maintainer hunting dead workarounds. Both are
  compositions of the v1 Excavate and Site Report.
- **Architectural maps.**
