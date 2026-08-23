# Roadmap

Versions are ordered, not dated. A feature moves up when the thing it waits on
exists, not when someone feels like it.

## v1 — one Excavate, done properly

Open one git repository as a Project, select a line range, and get a Finding:
what the code does, when it appeared, how it evolved, which tests touch it, and
why — where Evidence exists. Includes the original Project Assessment, provider
auth, MCP server, and Field Notes in SQLite.

## v1.1 — evidence to verified improvement

The version tracked by issue #21. It ships the complete agreed feature set:

- Project vocabulary and a distinctive, accessible redesign of every current
  workflow, including a hierarchical file tree and integrated activity center.
- Native provider connections plus an optional OpenCode bridge, authenticated
  OpenCode Free, Zen and Go, dynamic models, OAuth methods exposed by the
  provider, all published Go text protocols, persistence, and real contract and
  opt-in live tests.
- Project Overview and Project Assessment as the primary experience, with
  Opportunities linked through Findings and Evidence to candidate preparation.
- Strict Project and Candidate Quality Grades, provisional model critique for
  incomplete measurement, and baseline-to-candidate Improvement Receipts.
- Disposable-clone preparation, isolated approved commands, verification,
  manual GitHub Work Items, and explicitly enabled grade-threshold automatic
  publication. The active Project is never modified.

## v1.2 — more Evidence and more trackers

- **GitHub and GitLab PRs and issues as Evidence.** The real rationale often
  lives in review discussion rather than a commit message.
- **Staleness receipts from tracker state.** A closed linked issue becomes a
  checkable receipt instead of a commit-message heuristic.
- **Additional Work Item destinations.** GitLab, Linear, and Jira follow only
  after GitHub publication and idempotency are proven.
- **Field Note export.** Markdown out of the SQLite store for deliberate sharing
  or manual commit.

## v1.3 — the pull-request workflow

- **History-aware review for pull requests and commits.** Review comments remain
  Findings and cite Evidence under ADR-0001.
- **Pull-request creation from a verified candidate.** A user may promote a
  prepared candidate after reviewing its diff and Improvement Receipt.
- Automatic merge remains out: creating a reviewable proposal and merging code
  are different authority boundaries.

## v1.4 — language intelligence

- **Language parsers and first-party analyzers.** Track symbols and calculate
  language-aware metrics incrementally, one language at a time. Line ranges and
  project-provided tooling keep working everywhere else.
- **Coverage-based test linking**, only where the co-commit heuristic proves too
  noisy and the Project already has a reliable coverage command.

## v1.5 — larger workspaces

- **Background Project Assessment**, explicitly enabled and bounded by saved
  provider, command, and privacy permissions.
- **Multi-repository Projects and workspaces**, after ownership, grading, and
  tracker routing across repositories have concrete rules.

## Later, unscheduled

- **Automatic merge**, only after production evidence shows verified candidates,
  branch protections, rollback, and approval policies make it defensible.
- **Windows and Linux**, each with its own signing, secure storage, command
  isolation, and webview verification work.
- **Managed hosted fallback**, authenticated and explicit; never a shared or
  anonymous secret hidden inside the app.
- **Jump in from outside** from a file path, repository link, or terminal command.
- **Orientation and cleanup flows** composed from Project Assessment,
  Opportunities, Excavate, and Findings.
- **Architectural maps.**
