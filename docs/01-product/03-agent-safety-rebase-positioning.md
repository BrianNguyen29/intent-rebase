# Agent Safety Rebase Positioning

## Product Name

**Agent Safety Rebase** is the product positioning for the **Intent Rebase Engine (IRE)** repository.  
The repository name, crate names, and package identifiers remain unchanged.

## What is an Agent?

In this context, an **Agent** is any automated decision-maker or workflow runner that operates downstream of the control plane. This includes, but is not limited to:

- Long-running AI/ML pipelines
- Automated ops workflows
- Policy-driven automation
- Multi-step approval or compliance runners

It is intentionally broad: if a system makes decisions or executes workflows based on intent, it is an agent downstream of IRE.

## What Agent Safety Rebase Does

Agent Safety Rebase provides the control-plane layer for intent change in agent workflows:

- **Policy / config rebase** — detect and safely apply changes to policies, budgets, or constraints without a full restart
- **Workflow migration / rebase** — compute impact, salvage valid work, invalidate stale artifacts, and resume from a valid checkpoint
- **Multi-tenant compliance automation foundation** — tenant-scoped audit, approval, compensation, and forensic evidence

## What Agent Safety Rebase Is Not

To avoid scope creep and mis-positioning:

- **Not an LLM gateway** — IRE does not route, rate-limit, or proxy LLM calls
- **Not an agent runtime** — IRE does not execute agent logic or host agent processes
- **Not a tool-call executor** — IRE does not invoke tools or manage tool registries
- **Not an MCP bridge** — IRE does not act as a Model Context Protocol intermediary

## Relationship to Intent Rebase Engine

Agent Safety Rebase is a product-positioning name, not a repository or package rename. All crates, APIs, ADRs, and event contracts continue to use the **Intent Rebase Engine** name.

## Status

Non-production / integration-ready. Bounded slices have been delivered through Phase 3. Production readiness requires closure of the Phase 3 exit gate and external SRE/security sign-off.
