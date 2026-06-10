# Product Thesis

## Product Statement

**Intent Rebase Engine (IRE)** is the runtime/control plane layer that manages changes to user intent while agent workflows are running. IRE detects meaningful intent changes, determines their impact on plans, outputs, approvals, side effects, and memory, then produces a **rebase plan** so the workflow can be safely repaired, audited, and resumed from the nearest valid state.

## Market Problem

Agent systems are strengthening in three directions:
- tasks lasting from many minutes to many hours
- many sub-agents / many tools / many side-effect steps
- spec-driven workflows and human-in-the-loop

The biggest breakpoint is not just hallucination, but **system-level intent drift**:
- users change their minds mid-flight
- policy / budget / approval boundary changes
- spec / ticket / PR requirements change
- the system keeps running on the old intent or resets everything

This gap leads to:
- code going in the wrong direction
- support/ops actions using stale policy
- approvals no longer being valid
- memory/context retained in the wrong place
- vague root-cause when an incident occurs

## Positioning

IRE is:
- **version control for intent**
- **change impact engine for agent workflows**
- **repair / compensation orchestrator** for intent changes

IRE is not:
- a generic LLM gateway
- a full agent framework
- a pure workflow engine
- a pure memory database
- a pure observability tool

## Core Values

### 1. Reduce reset cost
Do not rerun the entire workflow when intent changes only locally.

### 2. Reduce risk of wrong action
When intent changes, approvals, policies, and side effects are re-evaluated structurally.

### 3. Increase reliability
Every output has provenance tied to an intent version.

### 4. Increase agent team productivity
Agents can salvage analysis, tests, artifacts, and only rerun affected parts.

### 5. Increase human control
Operators can clearly see:
- what change just happened
- which parts remain valid
- which parts are invalid
- which parts need review / compensation / re-approval

## Initial Deployment Wedge

The most suitable verticals for MVP:
- AI coding / software delivery
- internal ops workflows
- document review / policy-aware agents
- long-running research pipelines

In the short term, prioritize **coding agents** because:
- intent often changes mid-flight
- artifacts have clear structure: plan, patch, tests, PR, approvals
- ROI is easy to measure: reduced rerun, reduced churn, reduced spec drift

## Success Statement

An IRE production system is successful when:
- intent changes are formalized, diffable, and traceable
- workflows are not reset arbitrarily
- approvals/policies are re-evaluated when needed
- side effects are classified as compensatable/non-compensatable
- incidents can be replayed and explained
