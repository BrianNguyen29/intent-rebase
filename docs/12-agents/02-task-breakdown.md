# Task Breakdown for AI Agents

## Epic 1 — Intent Registry
- Create DB migrations for intents, intent_versions, intent_clauses
- Implement create intent endpoint
- Implement create version endpoint
- Implement get current/head/version history
- Add optimistic concurrency checks

## Epic 2 — Semantic Diff
- Implement structured diff for scope/constraints/acceptance/authority
- Implement severity assignment rules
- Implement confidence and manual-review triggers
- Add diff API
- Add regression fixtures

## Epic 3 — Graph and Impact
- Create graph nodes/edges storage
- Build artifact/approval/side effect edge ingestors
- Implement impact traversal
- Implement classification output

## Epic 4 — Rebase Planner
- Build preview-only planner
- Add decision classes A–E
- Add checkpoint selection heuristics
- Add approval revalidation hooks

## Epic 5 — Adapter
- Define capability contract
- Implement chosen runtime adapter
- Support pause/resume/checkpoint lookup
- Support apply preview -> apply transitions

## Epic 6 — Console
- Intent version history
- Semantic diff view
- Rebase preview page
- Workflow timeline
- Approval stale alerts

## Epic 7 — Audit and Replay
- Append-only audit pipeline
- Forensic export endpoint
- Timeline UI
