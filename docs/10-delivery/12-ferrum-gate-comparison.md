# Intent Rebase Engine vs. Ferrum-Gate — Positioning Comparison

> **Purpose:** Clarify the relationship between intent-rebase and ferrum-gate for teams evaluating or working across both repositories. This is a repo-local comparison only — ferrum-gate is not a dependency of intent-rebase.

---

## Overview

| Dimension | Intent Rebase Engine (intent-rebase) | Ferrum-Gate |
|-----------|-------------------------------------|-------------|
| **Primary role** | Control plane for intent lifecycle management | Execution gate for tool-call governance |
| **What it controls** | Intent versioning, semantic diff, dependency graph, approval invalidation, compensation | Tool-call execution, policy enforcement, audit trail for LLM/agent interactions |
| **Scope** | `R0–R3` (intent lifecycle: create → diff → rebase → apply → compensate) | `MCP bridge`, ledger, capability registry |
| **Key question it answers** | "Should this intent change trigger a rebase, and if so, what compensation is needed?" | "Should this tool call execute given current policy state?" |

---

## Architectural Separation

### Intent Rebase Engine — Control Plane

Intent Rebase Engine operates at the **intent layer**:

```
User Intent (v1) → Semantic Diff → Dependency Graph
                                      ↓
                Impact Classification → Approval Invalidation
                                      ↓
                    Rebase Decision → Compensation Plan
                                      ↓
                    User Approval → Rebased Execution (v2)
```

It does **not** execute tools or manage runtime execution. It determines *what needs to happen* when intent changes.

### Ferrum-Gate — Execution Gate

Ferrum-Gate operates at the **tool-call execution layer**:

```
Tool Call Request → Policy Evaluation → [Allow|Deny|Audit]
                                        ↓
                              MCP Bridge (if applicable)
                                        ↓
                              Ledger (immutable audit trail)
```

It does **not** understand intent semantics or compute semantic diffs. It enforces *whether a tool call is permitted* at execution time.

---

## Capability Comparison

| Capability | Intent Rebase Engine | Ferrum-Gate |
|-----------|---------------------|-------------|
| Intent schema + versioning | ✅ Core | — |
| Semantic diff (intent versions) | ✅ Core | — |
| Dependency graph | ✅ Core | — |
| Approval invalidation on change | ✅ Core | — |
| Compensation planning | ✅ Core | — |
| Tool-call policy evaluation | — | ✅ Core |
| MCP bridge | — | ✅ Core |
| Immutable execution ledger | — | ✅ Core |
| Capability registry | — | ✅ Core |
| Rebase execution | ✅ (via Temporal adapter) | — |
| Policy snapshot + revalidation | ✅ | — |

---

## Concrete Example

**Scenario:** A user updates an intent to remove field `X`, which previously approved an external API call.

**With Intent Rebase Engine:**
1. User submits intent v2 (without field `X`).
2. Rebase engine computes semantic diff → field `X` removed.
3. Dependency graph shows `X` → `Artifact Y` → `Approval Z`.
4. Approval `Z` is marked invalid; `Artifact Y` is flagged for quarantine consideration.
5. Compensation plan proposes: revoke approval `Z`, notify downstream.
6. User approves → rebase applied → downstream systems notified.

**Ferrum-Gate does not participate** in this flow because no tool call is being executed — this is intent-level governance.

**Scenario:** A tool call requests access to `delete_artifact(artifact_id=Y)`.

**With Ferrum-Gate:**
1. Tool call arrives at gate.
2. Policy evaluated: does caller have `delete_artifact` permission?
3. MCP bridge checks capability registry.
4. Ledger records the call attempt.
5. Decision: allow/deny.

**Intent Rebase Engine does not participate** in this flow because no intent change is occurring — this is execution-level governance.

---

## When Both Systems Interact

In a system where both repositories are deployed:

```
┌─────────────────────────────────────────────────────────────┐
│                    User Intent Layer                        │
│                   (Intent Rebase Engine)                    │
│   Intent v1 ──diff──▶ Rebase Decision ──▶ Compensation     │
└─────────────────────────────┬───────────────────────────────┘
                              │ (when rebase triggers external calls)
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                  Tool Execution Layer                       │
│                      (Ferrum-Gate)                         │
│   Tool Call ──policy──▶ MCP Bridge ──▶ Ledger              │
└─────────────────────────────────────────────────────────────┘
```

The interaction point is **downstream of a rebase decision**: when Intent Rebase Engine determines that a rebase requires executing a compensating action, and that action involves a tool call, Ferrum-Gate can serve as the execution gate for that tool call.

**Important:** This interaction is not automatic. Intent Rebase Engine does not call Ferrum-Gate directly in the current implementation. If tighter integration is desired, a future phase would need to add Ferrum-Gate as a policy executor in the compensation action execution path.

---

## What Each System Does NOT Do

### Intent Rebase Engine does NOT:
- Execute tool calls directly (delegates to Temporal/runtime adapter)
- Evaluate fine-grained tool-call permissions
- Manage MCP bridge connections
- Maintain an immutable execution ledger for every tool call

### Ferrum-Gate does NOT:
- Compute semantic diffs between intent versions
- Understand intent dependency graphs
- Make approval invalidation decisions
- Generate compensation plans

---

## Repository Relationship

- **Not a dependency:** intent-rebase does not import or depend on ferrum-gate.
- **Not a superset:** Each system has distinct, non-overlapping scope.
- **Potential integration point:** Future phases could wire Ferrum-Gate as a policy executor in Intent Rebase Engine's compensation action execution path (see [deferral register](./10-phase-2b-residual-risk-deferral-register.md), D-06 for notification delivery context).
- **Independent deployment:** Each can be operated standalone or together.

---

## Status Vocabulary

Both systems use the same normalized vocabulary for phase and deferral status:

| Status | Meaning |
|--------|---------|
| Open | Work not started |
| In Progress | Work underway; bounded slice delivered |
| Bounded Delivered | Bounded slice done; full scope deferred |
| Conditionally Complete | Phase exit passed with explicit deferrals |
| Closed | All obligations met; deferral resolved |

See [Phase 2b Residual Risk & Phase 3 Deferral Register](./10-phase-2b-residual-risk-deferral-register.md) for intent-rebase deferral details.

---

## Related Docs

- [System Overview](../02-architecture/01-system-overview.md) — Intent Rebase Engine architecture
- [Compensation Model](../03-spec/05-compensation.md) — Intent Rebase Engine compensation design
- [Phase 3 Hardening Plan](./05-phase-3-hardening.md) — Current phase status
