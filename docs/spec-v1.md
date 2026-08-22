# Subsurface v1

## Problem Statement

You are about to change a line of code and you do not know what you are looking
at. It has a strange guard, an unexplained retry, a magic constant, a comment
that says "don't remove this". Nobody who wrote it is on the team, `git blame`
gives you a commit called `fix`, and the only honest options are to leave it
alone forever or to change it and find out in production.

The rationale is not gone. It is in the repository — in what changed alongside
it, in the test that appeared the same day, in the doc that was edited in the
same commit. It is just not reachable in a form anyone will actually go and
read, so people either cargo-cult the code or delete it and cause an incident.

The same problem exists one level up. Nobody can point at which parts of a
codebase are undocumented, which workarounds are dead, or which regions nothing
tests, so cleanup is guesswork and onboarding is folklore.

## Solution

Subsurface is a local-first desktop app. You open a repository as a **Site**.
You select a region of code and **Excavate** it. You get a **Finding**:

- **What and when** — what the code does, when it appeared, how it evolved, and
  which tests touch it. Derived from git. Always present.
- **Why** — a plain-language rationale, but only when Subsurface can cite
  **Evidence** for it. When nothing recorded the why, the Finding says exactly
  that and asserts nothing.

Every claim carries **Confidence**: `Stated`, `Inferred`, or `None`. It is
assigned by rule from the shape of the Evidence, never by the model rating
itself, because the whole product is worthless if it is confidently wrong about
whether code is safe to delete.

Alongside the single Excavate, v1 ships the **Site Report**: the same engine run
across the whole repository, read-only. It names the regions with no recorded
rationale, the workarounds with a receipt saying they are dead, and the code
nothing tests. It never proposes an edit.

Findings you keep become **Field Notes**. Your agents reach the same engine and
the same Field Notes over an MCP server the app exposes, so a dig you do by hand
answers your agent's question later, and vice versa.

## User Stories

### Opening a Site

1. As a developer, I want to open a local git repository as a Site, so that I can investigate the code in it.
2. As a developer, I want opening a large repository to complete in seconds rather than minutes, so that I can start working immediately instead of waiting on a full index.
3. As a developer, I want Subsurface to tell me what it has and has not indexed yet, so that I know whether an answer is complete.
4. As a developer, I want my recently opened Sites listed on launch, so that I can return to an investigation without navigating a file picker.
5. As a developer, I want a clear error when I point Subsurface at a directory that is not a git repository, so that I am not left guessing why nothing loaded.
6. As a developer, I want Subsurface to work on a repository with no remote configured, so that I can investigate code that only exists on my machine.
7. As a developer working in a shallow clone, I want to be told that history is truncated, so that I do not read a partial history as the whole story.

### Excavating a selection

8. As a developer, I want to select a range of lines and press Excavate, so that I can find out why that specific code exists.
9. As a developer, I want the Finding to tell me what the selected code currently does, so that I can confirm I am reading it correctly before I judge its history.
10. As a developer, I want to know which commit first introduced the selected code, so that I can anchor everything else to a point in time.
11. As a developer, I want to see how the code changed between then and now, so that I can tell whether the original reason still applies.
12. As a developer, I want the selection followed through file renames and moves, so that history does not stop at the point the file was reorganised.
13. As a developer, I want the selection followed even when the code was reindented or reformatted, so that a whitespace commit does not erase the trail.
14. As a developer, I want to Excavate a region of a config file, a SQL migration, or a shell script, so that the tool works on the parts of the repo that are not application code.
15. As a developer, I want to Excavate a single line, so that I can ask about one magic constant without selecting the block around it.
16. As a developer, I want to cancel an Excavate that is taking too long, so that I am never stuck waiting on a hot file.
17. As a developer, I want to Excavate the same selection again later and get the saved Finding instantly, so that I do not pay for the same dig twice.
18. As a developer, I want to force a fresh Excavate of a selection I have dug before, so that I can get an updated answer after new commits land.

### The Finding and its Evidence

19. As a developer, I want every claim in a Finding to link to the specific commit, test, or doc it came from, so that I can check the reasoning myself.
20. As a developer, I want to click a piece of Evidence and see the actual diff, so that I do not have to trust a summary of it.
21. As a developer, I want the Finding to say plainly when no rationale was recorded, so that I know the absence of an answer is a real result and not a failure.
22. As a developer, I want Confidence shown as `Stated`, `Inferred`, or `None`, so that I can tell a documented reason from a reconstructed guess at a glance.
23. As a developer, I want to know why a Finding got the Confidence it did, so that I can judge whether to trust it.
24. As a developer, I want the What/When half of a Finding even when the Why half is empty, so that an Excavate is never a wasted trip.
25. As a developer, I want to see which tests changed alongside the selected code, so that I know what protects it before I change it.
26. As a developer, I want to be told that test linkage is a heuristic and can be wrong, so that I do not treat a coincidence as coverage.
27. As a developer, I want to see documentation edited in the same commits as the code, so that I find the design note somebody actually wrote.
28. As a developer, I want to see the Timeline of the selected region, so that I can read its evolution as a sequence rather than a pile of commits.
29. As a developer, I want to see how much Evidence was considered and how much was left out, so that I know the answer is based on a sample and what kind.
30. As a developer, I want a Finding to name the commits it excluded from the budget, so that I can look at one myself if I suspect it mattered.

### Staleness

31. As a maintainer, I want to be told when a workaround has a receipt saying it is dead, so that I can remove code safely instead of leaving it forever.
32. As a maintainer, I want the receipt itself shown — the closed issue, the dependency upgrade past the bad version — so that I can verify the claim before deleting anything.
33. As a maintainer, I want Subsurface to stay silent about staleness when it has no receipt, so that I am never nudged into deleting something on a hunch.
34. As a maintainer, I want old, rarely-touched code with no rationale to be described as exactly that rather than as dead, so that age is not mistaken for evidence.

### Site Report

35. As a maintainer, I want a repository-wide report of what has no recorded rationale, so that I can see where the knowledge gaps are without digging one line at a time.
36. As a maintainer, I want the report to list workarounds with a receipt saying they are dead, so that I have a starting point for cleanup.
37. As a maintainer, I want the report to list regions that no test appears to touch, so that I know where changes are riskiest.
38. As a maintainer, I want to click any entry in the report and land in the full Finding for it, so that the report is a way in rather than a dead end.
39. As a maintainer, I want to filter the report by directory or path, so that I can look at the subsystem I own.
40. As a maintainer, I want the report to say when it was generated and against which commit, so that I do not act on a stale picture.
41. As a maintainer, I want to regenerate the report on demand, so that I can refresh it after merging work.
42. As a maintainer, I want the report to be read-only, so that it never becomes another task list to maintain.
43. As a maintainer, I want the report's cost and duration made visible before it runs, so that I can decide whether to run it on a huge repository.

### Field Notes

44. As a developer, I want to save a Finding as a Field Note, so that an investigation I did once is not lost.
45. As a developer, I want to add my own notes to a Field Note, so that I can record what I concluded beyond what Subsurface found.
46. As a developer, I want to browse and search my Field Notes for a Site, so that I can find an investigation I remember doing.
47. As a developer, I want Field Notes stored outside my repository, so that investigating a codebase never produces a diff in it.
48. As a developer, I want Field Notes kept per Site, so that opening a different repository does not mix histories.
49. As a developer, I want to delete a Field Note, so that I can discard an answer I no longer believe.

### Providers and privacy

50. As a developer, I want to paste an API key and a base URL for any OpenAI-compatible provider, so that I can use whatever I already pay for.
51. As a developer, I want presets for OpenAI, Grok, OpenRouter, and OpenCode Zen, so that I do not have to look up base URLs.
52. As a developer, I want to sign in with OAuth where the provider offers it, so that I do not have to manage a key at all.
53. As a developer, I want my key stored in the OS keychain rather than a config file, so that it is not sitting in plaintext on my disk.
54. As a developer, I want to type any model name rather than pick from a list, so that a model launching or being retired is not my problem.
55. As a developer, I want to point Subsurface at a local Ollama instance, so that I can investigate code that cannot leave the machine.
56. As a developer, I want to see exactly what would be sent to a provider before it is sent, so that I can decide with full information.
57. As a developer, I want indexing, blame, rename-following, and Timelines to run entirely locally regardless of provider, so that only the narrowed evidence for one Excavate ever leaves.
58. As a developer, I want a clear, actionable error when a provider rejects my key or rate-limits me, so that I can fix it rather than guess.
59. As a developer, I want to switch providers without losing my Field Notes, so that my investigation history outlives my billing decisions.
60. As a security-conscious developer, I want a mode where no network call is possible at all, so that I can demonstrate compliance while auditing.

### Agents over MCP

61. As a developer using Claude Code, I want my agent to ask Subsurface why a region of code exists, so that it reasons from the repository's real history instead of guessing.
62. As a developer, I want my agent to receive a structured Finding rather than prose, so that it can act on the Confidence and the Evidence separately.
63. As a developer, I want my agent's question answered from an existing Field Note when one exists, so that the answer is instant and consistent with what I saw in the app.
64. As a developer, I want a dig triggered by my agent to appear in the app's Field Notes, so that the two halves accumulate one shared history.
65. As a developer, I want a clear "Subsurface is not running" error from the MCP tool, so that my agent fails understandably rather than hanging.
66. As a developer, I want Subsurface to be fully usable with no agent connected, so that it is a product and not a plugin.
67. As a developer, I want my existing skills to keep working with Subsurface uninstalled, so that adopting it is not a commitment.
68. As a developer, I want to see in the app when an agent is connected and what it has asked, so that I am not surprised by activity I did not initiate.

### Trust

69. As a developer, I want Subsurface to never propose or write a code change, so that it stays an instrument I read rather than an actor I supervise.
70. As a developer, I want to be able to tell, for any sentence Subsurface shows me, whether a human wrote it or the tool reconstructed it, so that I never mistake reconstruction for record.

## Implementation Decisions

### The single seam

Everything routes through one function:

```
excavate(site, path, line_range, provider) -> Finding
```

The UI's Excavate button, the MCP tool, and the Site Report all call it. The
Site Report is that function applied across a Site, not a second engine. This is
the only seam tests hook into, and it is deliberately the only one.

### Modules

- **Site** — opening a repository, the on-open index, and the handle everything
  else takes. On open it builds only the cheap things: the commit graph, file
  paths, and rename links. Expensive history walks happen per Excavate.
- **Evidence** — walks git for a selection and returns candidate Evidence.
  Follows a line range through history with `git log -L`, through renames, and
  across whitespace-only changes. Also surfaces test files and docs changed in
  the same commits.
- **Budget** — ranks candidate Evidence by how much of the selection each commit
  touched, takes the top N that fit the configured context window, and records
  what was excluded. Never silently truncates; the excluded set is part of the
  Finding.
- **Finding** — assembles the result. Builds the What/When half from git with no
  provider involved, and the Why half from the provider, gated on Evidence.
- **Confidence** — assigns `Stated` / `Inferred` / `None` by rule, from the shape
  of the Evidence, not from the provider. Pure logic, no IO.
- **Provider** — a trait with a single method. Real providers and the test fake
  are implementations of the same trait.
- **Store** — SQLite in the OS app-data directory, keyed by Site. Holds Field
  Notes and cached Findings. Nothing is written into the user's repository.
- **MCP** — an in-process server exposing the excavate tool, running only while
  the app is running.
- **Report** — the Site Report: a wide run of the same engine, plus filtering by
  path and a generated-at commit stamp.

### Confidence rules

Assigned by rule, in order:

- `Stated` — a commit message, doc, or code comment in the Evidence states the
  rationale in words.
- `Inferred` — no statement exists, but a rationale is reconstructable from what
  changed alongside the selection.
- `None` — no rationale recorded. The Finding says so and asserts nothing.

The provider never assigns Confidence and is never asked to rate itself. The
What/When half of a Finding carries no Confidence at all.

### Staleness

A region is flagged stale only on a receipt: a linked issue that is closed, or a
dependency upgraded past the version the code worked around. Age, churn, and
TODO comments are not receipts and produce no flag. Where a receipt exists, the
receipt is shown alongside the flag.

### Providers

Two auth paths, both in v1. A pasted key plus base URL and model name covers any
OpenAI-compatible endpoint, with presets for OpenAI, Grok, OpenRouter, and
OpenCode Zen, and a local Ollama endpoint as the offline path. OAuth ships for
providers that offer it. Keys live in the OS keychain. No curated model list —
the model is a text field with suggestions.

Before any request, the app can show the exact payload. Indexing, blame, rename
following, and Timeline construction never involve a provider.

### Storage

SQLite in app data, one database per machine, rows keyed by Site. Cached
Findings are invalidated when the commit the Site is at changes in a way that
touches the excavated path.

### Platform

Tauri with a Rust backend, macOS only for v1. The engine crate has no dependency
on the shell, so it is callable from the MCP server and from tests without
starting a UI.

### Milestones

Five, each gated on a runnable check. Ticket numbers are GitHub issues on
`vraj-ai/subsurface`, all sub-issues of the spec issue.

- **M1 Foundations** (#2, #3, #4, #8) — gate: the engine crate's tests run with
  no UI present and no network touched.
- **M2 Evidence engine** (#5, #6, #7, #11) — gate: rename, reformat and merge
  fixtures return the full trail, and each Confidence level is produced by a
  fixture built to trigger exactly it.
- **M3 First Finding** (#9, #12, #14) — gate: a fixture whose commits all say
  "fix" and "wip" produces a Finding that asserts nothing and says so.
- **M4 Product surface** (#13, #15, #16) — gate: select, Excavate and read works
  end to end on macOS, and an agent-triggered dig appears in Field Notes.
- **M5 Site Report and OAuth** (#10, #17) — gate: report entries match a direct
  Excavate, and one OAuth provider signs in end to end.

The two decisions taken against recommendation are both in M5 so that neither is
the critical path. If M5 slips, v1 ships without them.

## Testing Decisions

**What makes a good test here.** A test drives `excavate()` against a fixture
repository and asserts on the returned Finding — its claims, its Evidence links,
its Confidence, its excluded set. It never asserts on how the walk was
performed, which functions were called, or what SQL ran. If a test would break
when the ranking implementation is rewritten but the Finding is unchanged, it is
testing the wrong thing.

**Fixtures.** Small real git repositories built by a script from a declarative
description — commits, renames, reformats, test files landing alongside code,
issue references in messages. Real `.git` directories, not mocks of git, because
rename and whitespace following is exactly the behaviour most likely to be
wrong. Fixtures are generated deterministically with fixed timestamps and
authors so assertions are stable.

**The provider fake.** Provider is a trait with one method. Tests inject a fake
returning canned prose. Every assertion about Evidence selection, ranking,
budgeting, and Confidence is therefore deterministic and offline; no test in the
suite touches a network.

**What gets tested.**

- Line ranges followed through renames, moves, reformats, and merge commits.
- Evidence ranking and budget: correct top-N chosen, excluded set reported, never
  silently truncated.
- Confidence rules: each of `Stated` / `Inferred` / `None` produced from a
  fixture built to trigger exactly that case.
- A Finding with no rationale available says so, and asserts nothing — the single
  most important test in the suite.
- Staleness flagged only with a receipt; a fixture with old, churny, TODO-laden
  code and no receipt produces no flag.
- The What/When half is complete when the provider fails entirely.
- Store round-trip: a saved Field Note is returned identically, and cache
  invalidation fires when the excavated path changes.
- The MCP tool returns the same Finding as a direct call, and returns a saved
  Field Note when one exists.
- Site Report entries link to Findings that match a direct Excavate of the same
  region.

**Prior art.** None — this is the first code in the repository. These fixtures
and this seam are the prior art everything later follows.

## Out of Scope

Deferred, with the version each is scheduled for in `docs/roadmap.md`:

- **GitHub and GitLab PRs and issues as Evidence** (v1.1). The richest source of
  rationale, and the one that drags in network auth, rate limits, and per-forge
  APIs. v1 reads git, tests, and docs only.
- **PR review bot** (v1.2). A reviewer that reads history before commenting.
- **Symbol-level tracking** (v1.3). v1 follows line ranges; tracking a function
  or class through history needs a parser per language.
- **Coverage-based test linking** (v1.3, conditional). Only if the co-commit
  heuristic proves too noisy — it requires building and running arbitrary
  projects.
- **Actions on the Site Report** (v1.4). The report names problems; it does not
  track or assign them.
- **Background analysis** (v1.4), **multiple Sites in one workspace** (v1.4),
  **Field Note export** (v1.1), **jump-in from an editor or link**, **orientation
  and cleanup flows**, **architectural maps** (unscheduled).
- **Windows and Linux builds.** v1 is macOS only.
- **A headless daemon.** The MCP server runs only while the app is open.

Permanently out of scope, per `CONTEXT/architecture.md` Non-goals: Subsurface
never writes or suggests code edits, and is not an editor, a git client, a
coding assistant, or a documentation generator.

## Further Notes

Two decisions were taken against the recommendation, deliberately, and are
recorded with their cost:

- **OAuth in v1** (`docs/adr/0005`). The pasted-key path alone already covers
  every provider on the list, so OAuth buys sign-in experience at the price of a
  per-provider client registration, consent screen, and refresh flow, any of
  which can block a release on someone else's approval timeline. If it threatens
  the milestone, the key path ships alone and OAuth lands provider by provider.
- **Site Report in v1** (`docs/adr/0008`). Its ranking is only as good as the
  Findings beneath it, and v1 is where we first learn what a good Finding looks
  like. Expect the ranking to be wrong initially and to be re-tuned.

The name collides with Linus Torvalds' dive-log application. Different field,
milder collision than the previous name, accepted.

Vocabulary in this spec is defined in `CONTEXT.md`. Decisions it rests on are in
`docs/adr/0001` through `0008` and `CONTEXT/architecture.md`.
