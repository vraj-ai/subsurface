# Ship Goal — Subsurface v1.1

- Source: https://github.com/vraj-ai/subsurface/issues/21
- Stable slug: `21-subsurface-v1-1-evidence-to-verified-improvement`
- Main branch: `main`
- Worktree root: `/Users/vraj/Work/Projects/subsurface/CONTEXT/worktrees/21-subsurface-v1-1-evidence-to-verified-improvement`
- Pre-ship merge base: `db408c853ff3c7b7ce3fcb52f9833d8950a91c11`
- Maximum batch: 4
- Contributor: Codex CLI in Herdr pane `wV:p2`
- Orchestrator/final adversary: Grok CLI, Grok 4.6 High, pane `wV:p3`
- Council/reviewers: Claude Code Haiku 4.5 (`wV:p1`), Prime Agent Claude Sonnet 5 (`wV:p4`), Agy Gemini 3.7 Flash High (`wV:p5`)
- Approval state: approved; 42-item backlog created (35 code, 7 verify)

Council planning is complete. Codex on `wV:p2` was not prompted. The repository was not edited.

## Council evidence

| Target | Pane | Status | Evidence |
|---|---|---|---|
| Claude | `wV:p1` | Answered | Session `e18750c4-1e76-498b-8a85-fcde271c6d00`, Haiku 4.5. Plan extracted from Claude jsonl. Blocked twice on read/`cargo test --no-run`; both were approved as planning-only. |
| Prime Agent | `wV:p4` | Answered after recovery | First delivery hit OpenCode Go `401 Insufficient balance`. Same pane switched to already-available `anthropic/claude-sonnet-5` (session `01cb5c730f2c`) and returned a full plan. Herdr `agent prompt` cannot target this pane; the question was sent with `herdr pane run`. |
| Agy | `wV:p5` | Answered | Antigravity / Gemini 3.7 Flash High. Full plan captured from `herdr agent read`. |

Identical question: `/private/tmp/subsurface-issue-21-council-question.md`.

## Agreement

All three chose **7 milestones**, keep the **closed v1.1 set**, add **no later-version features**, sequence **Site → Project as expand-contract only**, require **one exact Cargo/Tauri command per item**, and name the same risks: **active-Project mutation**, **Go protocol mismatch**, **weakest-dimension leakage**, **publish idempotency**, and **durable activity across navigation**. All keep **MCP legacy Site input** through one compatibility release. All require `cargo test --workspace` plus a **production app build**, with visual/accessibility as a conjunctive release bar.

## Disagreement

| Point | Claude `wV:p1` | Prime `wV:p4` | Agy `wV:p5` | Selected |
|---|---|---|---|---|
| Smallest cut | Defers prep, publish, and UI after M4 | Walking-skeleton lifecycle; leaves auto-publish and extra providers as “breadth” | Full v1.1 in three internal partitions | Agy. Claude’s cut drops agreed features. Prime’s “unbuilt auto-publish” also drops agreed v1.1. |
| Rename contract | Grep-zero Site language in M1 | Contract last, after every caller migrated | Contract in M7 after UI wiring | Prime. M1 grep-zero fights expand-contract. |
| Test binaries | Reuse `m4`/`m5` plus `grep` wrappers | New `m6`–`m18` so existing `m2`–`m5` stay | New `m1`–`m7` names that collide with existing `m2`–`m5` | Prime numbering. |
| Lifecycle e2e | Split across M5/M6 | Dedicated `m17_lifecycle_e2e_test` after GitHub | Folded into M7 `cargo test --workspace` | Prime. Matches the spec’s one fixture-Project seam. |
| Item shape | Too many slices; some UI items have no exact Cargo command | Narrow tracers with `--exact` filters | Clean thematic tracers | Agy grouping + Prime commands. |

## Selected plan

Seven thematic milestones. New tests do **not** overwrite existing `m2_evidence_tests` … `workspace_tests`. Frontend checks are Rust assertions over `ui/` plus a release build. No new JS framework.

### M1 — Project foundation (expand)

Why: cheap open, persistence, MCP compat, and a durable activity record so later work is written once in Project vocabulary.  
Depends on: current `cargo test --workspace` (29 tests).

| ID | Type | Item | Depends | Command |
|---|---|---|---|---|
| M1-1 | code | Add `Project` / `Project::open()` beside `Site` | — | `cargo test -p subsurface-engine --lib project::tests` |
| M1-2 | code | SQLite expand: `project_path` beside `site_path`, idempotent Site-era migration | M1-1 | `cargo test -p subsurface-engine --test m6_project_migration_tests -- migrates_legacy_site_rows_without_loss --exact` |
| M1-3 | code | MCP accepts legacy `site_path`, emits Project | M1-1, M1-2 | `cargo test -p subsurface-engine --test m6_project_migration_tests -- mcp_accepts_site_emits_project --exact` |
| M1-4 | code | Tauri `open_project` / `assess_project` beside old commands | M1-1 | `cargo test -p subsurface-app --test command_contract_tests -- project_commands_are_registered --exact && cargo build -p subsurface-app --release` |
| M1-5 | code | Durable activity store skeleton | M1-2 | `cargo test -p subsurface-engine --test m7_activity_tests -- activity_survives_store_reopen --exact` |
| M1-6 | verify | Existing tests still pass | M1-1…M1-5 | `cargo test --workspace` |

**M1 gate:** `cargo test --workspace && cargo test -p subsurface-engine --test m6_project_migration_tests && cargo test -p subsurface-engine --test m7_activity_tests`

### M2 — Connections, OAuth, permissions

Why: one trust boundary and one local HTTP-fake technique reused later by GitHub.

| ID | Type | Item | Depends | Command |
|---|---|---|---|---|
| M2-1 | code | Local HTTP fake (stdlib, no new framework) | — | `cargo test -p subsurface-engine --test m8_provider_contract_tests -- local_http_fake_starts_and_stops --exact` |
| M2-2 | code | Per-connection Keychain + persisted non-secret prefs | M2-1 | `cargo test -p subsurface-engine --test m8_provider_contract_tests -- per_connection_keys_do_not_overwrite --exact` |
| M2-3 | code | Native OpenAI, xAI, OpenRouter, Zen, Go, Ollama, Custom; Go Completions/Responses/Messages | M2-1, M2-2 | `cargo test -p subsurface-engine --test m8_provider_contract_tests` |
| M2-4 | code | Dynamic models, editable fallback, optional OpenCode bridge with explicit reuse | M2-3 | `cargo test -p subsurface-engine --test m8_provider_contract_tests -- model_discovery_failure_keeps_model_field_editable --exact` |
| M2-5 | code | Real OAuth/device/callback/refresh/cancel against the fake | M2-1, M2-2 | `cargo test -p subsurface-engine --test m9_oauth_bridge_tests` |
| M2-6 | code | Payload preview, per-request default, per-Project always-allow, offline blocks sockets | M2-2 | `cargo test -p subsurface-engine --test m9_oauth_bridge_tests -- offline_mode_blocks_external_call --exact` |
| M2-7 | verify | Workspace still green | M2-1…M2-6 | `cargo test --workspace` |

**M2 gate:** `cargo test --workspace && cargo test -p subsurface-engine --test m8_provider_contract_tests && cargo test -p subsurface-engine --test m9_oauth_bridge_tests`

### M3 — Strict Quality Grade

Why: receipt-backed weakest-dimension rules must exist before Opportunities or publish can use them.

| ID | Type | Item | Depends | Command |
|---|---|---|---|---|
| M3-1 | code | 0–100 / A+–F, six dimensions, overall = min, Incomplete | — | `cargo test -p subsurface-engine --test m10_grade_tests -- lowest_dimension_and_incomplete_rules --exact` |
| M3-2 | code | Spec bands and hard caps (build/test/critical security → F; high-severity caps below A) | M3-1 | `cargo test -p subsurface-engine --test m10_grade_tests` |
| M3-3 | code | Per-Project overrides always disclosed | M3-2 | `cargo test -p subsurface-engine --test m10_grade_tests -- override_always_disclosed_in_receipt --exact` |
| M3-4 | code | Provisional Assessment cannot invent values, remove Incomplete, or auto-publish | M3-1, M2-3 | `cargo test -p subsurface-engine --test m10_grade_tests -- provisional_cannot_override_incomplete_or_hard_fail --exact` |
| M3-5 | verify | Workspace still green | M3-1…M3-4 | `cargo test --workspace` |

**M3 gate:** `cargo test --workspace && cargo test -p subsurface-engine --test m10_grade_tests`

### M4 — Project Assessment and Opportunities

Why: Assessment is the primary experience; detectors become Opportunities with inspectable ordering.

| ID | Type | Item | Depends | Command |
|---|---|---|---|---|
| M4-1 | code | Opportunity lifecycle Detected → Prepared → Verified/Failed → Published/Dismissed | — | `cargo test -p subsurface-engine --test m11_opportunity_tests -- opportunity_lifecycle_transitions --exact` |
| M4-2 | code | Dead-workaround, missing-rationale, test-gap, tooling receipts; model-proposed labeled | M4-1 | `cargo test -p subsurface-engine --test m11_opportunity_tests -- detectors_emit_linked_opportunities --exact` |
| M4-3 | code | Ordering by impact, verification, expected grade, effort, age | M4-1 | `cargo test -p subsurface-engine --test m11_opportunity_tests -- ordering_uses_separate_fields_not_composite_score --exact` |
| M4-4 | code | Assessment at exact commit, grade history, baseline deltas | M4-1, M4-2, M3-1 | `cargo test -p subsurface-engine --test m11_opportunity_tests -- project_assessment_at_commit_with_history --exact` |
| M4-5 | code | Preview, progress, cancel, partial labeled | M4-4 | `cargo test -p subsurface-engine --test m11_opportunity_tests -- assessment_preview_and_cancellation --exact` |
| M4-6 | verify | Workspace still green | M4-1…M4-5 | `cargo test --workspace` |

**M4 gate:** `cargo test --workspace && cargo test -p subsurface-engine --test m11_opportunity_tests`

### M5 — Disposable clone, isolation, receipts

Why: the no-mutation invariant is its own milestone.

| ID | Type | Item | Depends | Command |
|---|---|---|---|---|
| M5-1 | code | Disposable clone; active Project hash unchanged even on failure | — | `cargo test -p subsurface-engine --test m12_candidate_tests -- active_project_hash_unchanged_after_preparation --exact` |
| M5-2 | code | Allowlist, scrubbed env, network deny-by-default, bounds | M5-1 | `cargo test -p subsurface-engine --test m13_command_isolation_tests` |
| M5-3 | code | Prepare via optional OpenCode runtime; actionable unavailable state | M5-2, M2-5 | `cargo test -p subsurface-engine --test m12_candidate_tests -- no_runtime_yields_actionable_unavailable_state --exact` |
| M5-4 | code | Baseline then candidate; Improvement Receipt; Failed stays queued | M5-2, M5-3 | `cargo test -p subsurface-engine --test m14_receipt_tests` |
| M5-5 | verify | Workspace still green | M5-1…M5-4 | `cargo test --workspace` |

**M5 gate:** `cargo test --workspace && cargo test -p subsurface-engine --test m12_candidate_tests && cargo test -p subsurface-engine --test m13_command_isolation_tests && cargo test -p subsurface-engine --test m14_receipt_tests`

### M6 — GitHub Work Items and lifecycle e2e

Why: GitHub is the only destination; automation stays off by default.

| ID | Type | Item | Depends | Command |
|---|---|---|---|---|
| M6-1 | code | Infer remote; `gh` after permission; browser/device/token fallback | — | `cargo test -p subsurface-engine --test m15_github_publish_tests -- github_auth_and_destination_fallback_order --exact` |
| M6-2 | code | Work Item body + fingerprint idempotency | M6-1 | `cargo test -p subsurface-engine --test m15_github_publish_tests` |
| M6-3 | code | Manual editable preview; closed issue does not resolve Opportunity | M6-2 | `cargo test -p subsurface-engine --test m16_automation_tests -- closed_work_item_does_not_resolve_without_reassessment --exact` |
| M6-4 | code | Auto-publish off by default: category enable, proven example, min grade, Incomplete excluded, daily limit, countdown, cancel, log | M6-2, M6-3, M3-4 | `cargo test -p subsurface-engine --test m16_automation_tests` |
| M6-5 | code | One GitFixture lifecycle: Assess → prepare → verify → grade → receipt → publish | M1–M6 prior | `cargo test -p subsurface-engine --test m17_lifecycle_e2e_test` |
| M6-6 | verify | Workspace still green | M6-1…M6-5 | `cargo test --workspace` |

**M6 gate:** `cargo test --workspace && cargo test -p subsurface-engine --test m15_github_publish_tests && cargo test -p subsurface-engine --test m16_automation_tests && cargo test -p subsurface-engine --test m17_lifecycle_e2e_test`

### M7 — Explorer, settings, visual system, rename contract

Why: UI sits on a complete engine; Site is deleted from active language only after every caller migrated.

| ID | Type | Item | Depends | Command |
|---|---|---|---|---|
| M7-1 | code | Hierarchical tree, keyboard, ancestor-preserving filter, persisted expansion | M1-1 | `cargo test -p subsurface-engine --test m18_tree_tests` |
| M7-2 | code | Detail surface order Problem → Finding → Evidence → Candidate → Checks → Grade → Receipt → Publish | M7-1, M4, M5, M6 | `cargo test -p subsurface-app --test ui_asset_tests -- detail_surface_section_order_matches_spec --exact` |
| M7-3 | code | Settings: Connections, Privacy, Quality, Automation, Appearance | M7-2, M2-6, M6-4 | `cargo test -p subsurface-app --test ui_asset_tests -- settings_has_five_named_sections --exact` |
| M7-4 | code | Activity center UI; nav does not cancel work | M1-5, M7-3 | `cargo test -p subsurface-app --test ui_asset_tests -- activity_center_survives_navigation --exact` |
| M7-5 | code | OKLCH tokens, quality rail, contrast, focus, reduced motion, dependency-free Chrome visual fixtures | M7-2 | `cargo test -p subsurface-app --test ui_asset_tests -- tokens_css_defines_oklch_light_and_dark_and_passes_contrast_check --exact` |
| M7-6 | code | **Contract:** remove Site from active UI/commands/errors; keep MCP legacy input | all migrate batches | `cargo test -p subsurface-engine --test m6_project_migration_tests && cargo test -p subsurface-app --test ui_asset_tests -- no_site_labels_remain_in_ui --exact` |
| M7-7 | verify | Production build + visual capture | M7-1…M7-6 | `cargo test --workspace && cargo build -p subsurface-app --release && ./scripts/capture_visual_states.sh` |

**M7 gate:** `cargo test --workspace && cargo test -p subsurface-app --test ui_asset_tests && cargo build -p subsurface-app --release && ./scripts/capture_visual_states.sh`

T3 visual review of the captured light/dark, common/narrow screenshots is **not** replaced by a green shell command.

## Project rename (expand-contract only)

| Phase | What | Command |
|---|---|---|
| Expand | M1-1, M1-2: `Project` and `project_path` beside Site | `cargo test -p subsurface-engine --test m6_project_migration_tests` |
| Migrate 1 | New engine modules written Project-native | `cargo test -p subsurface-engine --lib` |
| Migrate 2 | Tauri commands added beside old ones | `cargo build -p subsurface-app --release` |
| Migrate 3 | UI copy/IDs to Project | `cargo test -p subsurface-app --test ui_asset_tests -- no_site_labels_remain_in_ui --exact` |
| Contract | M7-6: delete active Site language; MCP `site_path` input remains | `cargo test -p subsurface-engine --test m6_project_migration_tests -- mcp_still_accepts_legacy_site_field_after_contract --exact` |

## Integration risks (ranked)

1. **M5** — command-runner escape or active-Project mutation. Prove hash + porcelain before/after, including failures.
2. **M1** — non-idempotent SQLite Site→Project migration loses Field Notes/pins. Must be safe on every open before M2 writes new columns.
3. **M2** — Go Completions/Responses/Messages mismatch; current OAuth test only mutates tokens.
4. **M3→M6** — grade/Incomplete leak into auto-publish.
5. **M6** — timeout-after-success duplicates GitHub issues without fingerprint recovery.
6. **M7** — splitting the 1624-line `index.html` breaks `frontendDist: "ui"` state.
7. **M4/M7** — one `Mutex<Connection>` blocking UI during long assessment/prep.

## Smallest viable cut

Not a feature drop. Internal partition only:

- **A:** M1–M3 (Project, connections, strict grades)
- **B:** M4–M6 (Assessment, Opportunities, isolation, receipts, GitHub, lifecycle e2e)
- **C:** M7 (tree, detail, settings, visual, rename contract)

Must not be deferred: Project migration, all native providers plus Go protocols, real HTTP contract tests, weakest-dimension + Incomplete, disposable-clone no-mutation, Failed receipts, fingerprint idempotency, auto-publish **off by default but implemented**, hierarchical tree, activity center, visual/a11y bar, MCP Site-input shim.

## Final success command

```bash
cargo test --workspace \
  && cargo test -p subsurface-engine --test m17_lifecycle_e2e_test \
  && cargo build -p subsurface-app --release \
  && ./scripts/capture_visual_states.sh
```

Then T3 composed-system + visual review. A backend-green, visually poor build fails issue 21.

No backlog was written. No Codex dispatch. Waiting for explicit approval before any implementation.
