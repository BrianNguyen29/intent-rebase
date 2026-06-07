# Intent Rebase Engine — Public Documentation Hub

> **Safety:** IRE is **not production-ready** and is not validated for
> production, sensitive, or customer-facing workloads. Use it only for
> local development, integration experimentation, and bounded study of
> the design. Do not rely on any setting, command, or example on this
> site as production hardening guidance. See
> [Status & Capabilities](./reference/status-and-capabilities.md) and
> the [Capability Support Matrix](./01-product/04-capability-support-matrix.md).

This page is the **public documentation hub** for Intent Rebase Engine
(IRE). It is the entry point for GitHub visitors, contributors, and
integrators.

---

## Start here

- [Quickstart](./getting-started/quickstart.md) — prerequisites, clone,
  fast verify, local stack, run the API.
- [Configuration](./getting-started/configuration.md) — environment
  variables and the `#[ignore]`'d test suites.
- [Development & Verification](./getting-started/development.md) —
  local commands and the smoke / heavy / manual split.

## Status & capability

- [Status & Capabilities](./reference/status-and-capabilities.md) —
  concise non-production status and the boundaries of safe use.
- [Capability Support Matrix](./01-product/04-capability-support-matrix.md)
  — per-capability bounded support vs. production status.
- [Agent Safety Rebase Positioning](./01-product/03-agent-safety-rebase-positioning.md)
  — formal product positioning and scope boundaries.
- [Product Thesis](./01-product/01-product-thesis.md),
  [Goals & Non-Goals](./01-product/02-goals-nongoals.md),
  [Use Cases](./01-product/04-use-cases.md) — additional product
  context.
- [Glossary](./01-product/05-glossary.md) — domain vocabulary.

## Architecture

- [System Overview](./02-architecture/01-system-overview.md) —
  high-level architecture, planes, and trust boundaries.
- [Components](./02-architecture/02-components.md) — per-component
  responsibilities and interfaces.
- [Trust Boundaries](./02-architecture/03-trust-boundaries.md),
  [Scaling Topology](./02-architecture/04-scaling-topology.md),
  [Deployment Models](./02-architecture/05-deployment-models.md) —
  additional architecture reference.

## Spec (intent, diff, graph, rebase)

- [Intent Model](./03-spec/01-intent-model.md) — versioned intent
  structure, change classification, modeling rules.
- [Semantic Diff](./03-spec/02-semantic-diff.md) — diff engine contract.
- [Dependency Graph](./03-spec/03-dependency-graph.md) — graph model.
- [Rebase Engine](./03-spec/04-rebase-engine.md) — rebase algorithm
  and plan structure.
- [Compensation](./03-spec/05-compensation.md),
  [Provenance](./03-spec/06-provenance.md) — supporting semantics.

## API

- [REST API notes](./04-api/01-rest-api.md) — design principles,
  high-level resource layout, error model, security requirements.
- [OpenAPI spec](./04-api/openapi.yaml) — **canonical** endpoint
  definitions, request / response schemas, and parameters.
- [Events](./04-api/02-events.md) — event contracts.
- [Webhooks](./04-api/03-webhooks.md) — webhook contract and examples.
- [Impact Report Examples](./04-api/impact-report-examples.md) and
  [Route ↔ OpenAPI Contract Map](./04-api/route-openapi-contract-map.md)
  — supporting API reference.

## Verification

- [Test Strategy](./11-quality/01-test-strategy.md) — local
  verification loop, ignored test policy, live-integration policy.
- [`scripts/verify-fast.sh`](../scripts/verify-fast.sh) — the fast
  local verify helper.

## Decisions and reference

- [ADR Pack](./13-adrs/README.md) — Architecture Decision Records
  driving the design.
- [Rationale & external patterns](./99-reference/01-rationale-and-external-patterns.md)
  — where the design ideas come from (Temporal, LangGraph, Anthropic
  harnesses, spec-driven dev, impact analysis, plan repair, event
  sourcing, compensation).

## Contributing, security, support

- [`CONTRIBUTING.md`](../CONTRIBUTING.md) — how to contribute.
- [`SECURITY.md`](../SECURITY.md) — security policy and reporting.
- [`.github/SUPPORT.md`](../.github/SUPPORT.md) — where to get help
  (there is **no SLA**).
- [`AGENTS.md`](../AGENTS.md) — deeper contributor / agent rules.
