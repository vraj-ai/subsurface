# Domain Docs

This repo is **single-context**.

- `CONTEXT.md` at the repo root — the glossary, and nothing else. No implementation
  details, no specs, no scratch notes.
- `docs/adr/` — Architecture Decision Records, numbered `0001-...`, `0002-...`.

Distinct from these, and owned by the `goals`/`ship` skills:

- `CONTEXT/architecture.md` — durable intent, locked decisions, invariants, non-goals.
- `CONTEXT/progress.md` — bounded derived milestone pointer.

## Consumer rules

- Read `CONTEXT.md` for vocabulary before writing a spec, ticket, or ADR, and use
  its terms exactly.
- Write an ADR only when a decision is hard to reverse, surprising without context,
  and the result of a real trade-off. If any of the three is missing, skip it.
