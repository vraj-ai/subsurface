# Issue Tracker

Issues for this repo live in **GitHub Issues** on `vraj-ai/subsurface`.

Use the `gh` CLI. The active `gh` account is `vraj-ai` (scopes: `gist`, `read:org`, `repo`, `workflow`).

## Operations

- **Create**: `gh issue create --title "..." --body-file <file> --label <label>`
- **Read**: `gh issue view <number> --json number,title,body,labels,state,assignees,comments`
- **List**: `gh issue list --label <label> --state open --json number,title,labels,assignees`
- **Comment**: `gh issue comment <number> --body-file <file>`
- **Close**: `gh issue close <number>`
- **Assign (claim)**: `gh issue edit <number> --add-assignee @me`

## Wayfinding operations

- **The map** is an issue labelled `wayfinder:map`.
- **Tickets** are GitHub **sub-issues** of the map (native parent/child), each labelled
  with one of `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling`,
  `wayfinder:task`.
- **Blocking** uses GitHub's native issue **dependencies** (`blocked by` / `blocks`),
  which render in the GitHub UI. Both sub-issues and dependencies are GraphQL-only —
  drive them with `gh api graphql`. If a call fails on permissions, fall back to a
  `Blocked by: #N` line in the ticket body and say so explicitly rather than silently.
- **Claim** = assign the ticket to `@me` before any work.
- **Frontier query**: open sub-issues of the map that are unassigned and have no
  open blockers.

## PRs as a request surface

**Off.** Pull requests are not part of the triage queue for this repo.
