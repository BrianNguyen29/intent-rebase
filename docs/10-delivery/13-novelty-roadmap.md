# Intent Rebase Engine — Novelty Roadmap

> **Purpose:** Propose differentiated feature extensions for intent-rebase that expand its core competency as the *intent-change lifecycle control plane* without duplicating ferrum-gate capabilities. Each candidate is evaluated against the product boundary: intent-rebase owns intent versioning, semantic diff, dependency graphs, approval invalidation, and compensation; ferrum-gate owns tool-call governance, MCP bridge, capability registry, and execution ledger.

---

## Anti-Overlap Gates

Before evaluating any candidate, it must pass these gates to avoid duplicating ferrum-gate:

| Gate | Ferrum-Gate Scope | intent-rebase Boundary |
|------|------------------|----------------------|
| **G1 — Tool-call execution** | Direct tool invocation, MCP bridge, ledger | Never executes tools; only plans compensation |
| **G2 — Policy evaluation at call time** | Runtime permit/deny for operations | Policy snapshots are version-pinned; not re-evaluated per call |
| **G3 — Capability registry** | What tools/capabilities exist | Which *intents* reference which artifacts/approvals |
| **G4 — Immutable execution ledger** | Every tool call logged | Intent state transitions and compensation actions logged |
| **G5 — R0–R3 scope** | MCP bridge, ledger, capability registry | Intent lifecycle: create → diff → rebase → apply → compensate |

**If a candidate primarily involves G1–G4, it belongs to ferrum-gate — do not propose here.**

---

## Prioritized Candidates (N1–N6)

### N1 — IRPaaS: Intent Replan Propagation as a Service
**Priority: 1 (tied highest ROI)** | **Phase: P4**

**What it is:** Expose intent rebase outcomes (invalidation cascade, compensation plans, approval impacts) as a first-class declarative API that downstream systems can subscribe to or query. Enables external schedulers, agents, and workflow engines to react to intent changes without embedding rebase logic.

**Why novel for intent-rebase:**
- Extends the control plane from internal state management to *propagating intent-change signals* to external consumers
- Uses existing rebase decision, compensation planning, and approval invalidation outputs as API contracts
- Complements (not duplicates) event streaming — event streaming tells *when* something changed; IRPaaS tells *what the impact is and what to do next*

**Ferrum-gate anti-overlap:**
- Ferrum-gate evaluates *can this tool call execute?* at call time
- IRPaaS answers *what intent changes affect my workflow and what compensation is needed?* at intent-change time
- No tool-call execution, no MCP bridge, no capability registry involved

**Why highest ROI (tied):** Eliminates tight coupling between intent-rebase and external systems. Each downstream system currently needs custom integration logic; IRPaaS provides a standard interface.

**Deliverables:**
- `GET /v1/intents/{intent_id}/impact-report` — what artifacts, approvals, and side effects are affected
- `GET /v1/intents/{intent_id}/propagation-status` — what downstream systems have acknowledged/reacted
- Webhook registration API for downstream subscriptions
- Propagation audit trail

**Phase:** P4 (after Phase 3 batches complete; depends on compensation planning and approval invalidation being stable)

---

### N2 — Cross-Workflow Intent Lineage
**Priority: 3 | Phase: P4**

**What it is:** Track and visualize how an intent's artifacts propagate across workflow boundaries — when an artifact created under Intent A is consumed by Intent B, record the lineage edge. Enables impact analysis when any intent in the chain changes.

**Why novel for intent-rebase:**
- Builds on the existing dependency graph but extends it *across workflow boundaries*
- The graph already tracks intra-intent dependencies; cross-workflow lineage adds inter-intent, inter-tenant edges
- Enables "what happens to downstream intents if I rebase this one?" questions

**Ferrum-gate anti-overlap:**
- Ferrum-gate has no concept of intent or workflow semantics
- Lineage is about *intent-change impact* not tool-call permission
- No capability registry, no MCP bridge, no execution ledger

**Deliverables:**
- Cross-workflow edge model and repository
- `POST /v1/graph/lineage-edges` and `GET /v1/graph/lineage?intent_id=X`
- Impact propagation API: "show me all workflows that consume artifacts from this intent"
- Lineage visualization in orchestration dashboard

**Phase:** P4 (depends on graph service being stable and Phase 3 Batch 3 tenant isolation tests passing)

---

### N3 — Intent Quality Scoring
**Priority: 2 | Phase: P4**

**What it is:** Score intent drafts on clarity, completeness, and rebase-friendliness before they enter the lifecycle. Produces a quality grade (A–D) with specific improvement suggestions. Reduces downstream rebase frequency caused by vague or poorly structured intents.

**Why novel for intent-rebase:**
- Complements semantic diff by evaluating *before* a change occurs
- Uses existing intent schema validation, dependency graph density analysis, and compensation planning history as signals
- Quality scores become inputs to the approval workflow (low-scoring intents require extra review)

**Ferrum-gate anti-overlap:**
- Ferrum-gate evaluates *tool-call correctness* at execution time
- Intent quality scoring evaluates *intent draft quality* at authoring time
- No tool-call governance, no MCP bridge, no capability registry

**Deliverables:**
- Intent quality model: clarity, specificity, dependency density, compensation predictability
- `POST /v1/intents/quality-score` (pre-commit validation)
- Quality score history per intent version
- Improvement suggestions API (read-only recommendations)

**Phase:** P4 (depends on compensation planning stability to predict rebase frequency)

---

### N4 — Compensation Simulation Engine
**Priority: 1 (tied highest ROI) | Phase: P3/P4**

**What it is:** Before applying a rebase, simulate the full compensation execution path — not just the plan but the *actual compensation outcomes* with mock/forecast data. Answers: "If I rebase from v1 to v3, what compensation actions will fire, how many will succeed, and what residual risk remains?"

**Why novel for intent-rebase:**
- Current compensation planning generates a *plan*; compensation simulation runs a *dry-run execution* with bounded mock executors
- Uses the existing four executor classes (Rollback, CounterAction, FollowupNotice, Escalation) but executes against a simulation mode
- Highest ROI because it surfaces rebase risk *before* approval, reducing emergency rollbacks

**Ferrum-gate anti-overlap:**
- Ferrum-gate gates *actual* tool calls in *actual* execution
- Compensation simulation runs *mock* compensation in *forecast* mode
- No MCP bridge, no capability registry, no real execution ledger

**Deliverables:**
- Simulation mode flag on `POST /compensation-actions/runs` and `POST /compensation-actions/orchestration-dry-run`
- Simulation report: action-by-action outcome forecast, success probability, residual risk
- `GET /intents/{intent_id}/rebase-simulation` — pre-rebase impact simulation
- Bounded mock executors for each of the four compensation action types

**Phase:** P3 (can start early; uses existing compensation architecture in simulation mode) / P4 (full integration)

---

### N5 — Drift Detection
**Priority: 4 | Phase: P4**

**What it is:** Detect when an intent's *actual state* diverges from its *expected state* — the artifact produced doesn't match what the intent prescribed, or the approval scope has drifted since approval was granted. Triggers rebase consideration or re-approval workflows.

**Why novel for intent-rebase:**
- Builds on existing approval invalidation and policy snapshot revalidation, but extends to artifact-level drift
- Uses side-effect ledger and artifact provenance to detect drift at runtime
- Drift detection feeds back into compensation planning

**Ferrum-gate anti-overlap:**
- Ferrum-gate detects *policy violations* at tool-call time
- Drift detection identifies *semantic drift* between intent and produced artifact
- No tool-call execution, no MCP bridge, no capability registry

**Deliverables:**
- Drift detection model: artifact-vs-intent alignment, approval scope drift
- Background drift scanner (periodic, not per-request)
- `GET /intents/{intent_id}/drift-report`
- Drift-triggered re-approval workflow (uses existing approval request API)

**Phase:** P4 (depends on side-effect ledger and artifact provenance being production-stable)

---

### N6 — Policy Simulation (What-If)
**Priority: 2 | Phase: P4**

**What it is:** Simulate the impact of rule-pack changes on existing active intents before deploying the new rule pack. Answers: "If I update rule pack v2.1 to v2.2, how many active intents will require re-approval, and which compensation actions will be reclassified?"

**Why novel for intent-rebase:**
- Uses existing policy snapshot architecture but applies a *new* rule pack to *existing* intent versions
- Complements policy snapshot revalidation (which compares same rule pack across time) with *rule pack migration what-if*
- Policy simulation is pre-deployment validation; revalidation is post-deployment monitoring

**Ferrum-gate anti-overlap:**
- Ferrum-gate enforces policy at *tool-call execution time*
- Policy simulation evaluates policy impact at *intent-change time*
- No tool-call execution, no MCP bridge, no capability registry

**Deliverables:**
- `POST /v1/policy-snapshots/simulate-upgrade` — apply new rule pack to existing intent versions
- Rule-pack impact report: affected intents, re-approval requirements, compensation reclassification
- `GET /v1/rule-packs/{pack_id}/impact-preview`
- Rollback simulation (what-if revert)

**Phase:** P4 (depends on policy snapshot and rule pack versioning being stable)

---

## Candidate Summary Table

| Candidate | Novelty Rationale | Ferrum-Gate Separation | Phase |
|-----------|-------------------|----------------------|-------|
| **N1 IRPaaS** | Intent-change signal propagation to external systems | G1–G4: No tool-call execution, no MCP/capability/ledger | P4 |
| **N2 Cross-workflow Lineage** | Inter-intent, inter-workflow dependency tracking | G3: Intent semantics only; no capability registry | P4 |
| **N3 Intent Quality Scoring** | Pre-commit intent draft evaluation | G1–G4: No tool-call governance | P4 |
| **N4 Compensation Simulation** | Pre-rebase mock execution with outcome forecasting | G1–G4: Mock compensation, not real tool calls | P3/P4 |
| **N5 Drift Detection** | Artifact-intent alignment monitoring | G1–G4: Semantic drift, not policy violations | P4 |
| **N6 Policy Simulation** | Rule-pack migration what-if analysis | G1–G4: Policy impact at intent time, not call time | P4 |

---

## Relationship to Phase 3 Open Items

| Novelty Candidate | Phase 3 Dependency |
|------------------|-------------------|
| N1 IRPaaS | Depends on: compensation planning, approval invalidation stable |
| N2 Cross-workflow Lineage | Depends on: graph service stable, P3 tenant isolation |
| N3 Intent Quality Scoring | Depends on: compensation planning history available |
| N4 Compensation Simulation | Uses: existing compensation architecture in simulation mode (can parallelize) |
| N5 Drift Detection | Depends on: side-effect ledger production-stable |
| N6 Policy Simulation | Depends on: policy snapshot and rule pack versioning stable |

---

## Out of Scope (Ferrum-Gate Territory)

The following are explicitly **not** novelty candidates for intent-rebase:

| Feature | Why Ferrum-Gate |
|---------|----------------|
| MCP bridge | Tool-call protocol translation belongs to ferrum-gate |
| Capability registry | What tools exist belongs to ferrum-gate |
| PDP (Policy Decision Point) per tool call | Runtime permit/deny belongs to ferrum-gate |
| Immutable execution ledger per tool call | Every tool call logged belongs to ferrum-gate |
| R0–R3 governance | These phases cover MCP bridge, ledger, capability registry scope |
| Adapter layer for external tool ecosystems | Tool adaptation belongs to ferrum-gate |

---

## Related Docs

- [Intent Rebase Engine vs. Ferrum-Gate Comparison](./12-ferrum-gate-comparison.md) — Positioning clarity between intent-rebase and ferrum-gate
- [Current Project Status](./00-current-status.md) — Phase 3 delivery state and open items
- [System Overview](../02-architecture/01-system-overview.md) — Control plane architecture
- [Compensation Model](../03-spec/05-compensation.md) — Compensation design foundations
- [Phase 3 Hardening Plan](./05-phase-3-hardening.md) — Current phase plan
