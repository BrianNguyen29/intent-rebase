---
name: Feature request
about: Propose a new bounded slice, capability, or documentation improvement
title: "[feature] "
labels: []
assignees: []
---

> **Before opening a feature request:** please read
> [Status & Capabilities](https://github.com/BrianNguyen29/intent-rebase/blob/main/docs/reference/status-and-capabilities.md)
> and the
> [Capability Support Matrix](https://github.com/BrianNguyen29/intent-rebase/blob/main/docs/01-product/04-capability-support-matrix.md).
> IRE is delivered in **bounded slices**. Feature requests that imply
> CI-green, production-ready, or external-sign-off status will be
> redirected.

## Summary

<!-- One paragraph: what capability or bounded slice are you proposing, and
     why? -->

## Scope and ADR alignment

<!-- Best-effort mapping. Drop a note if you're not sure. -->

- Area: <!-- e.g. intent schema, API, graph rules, ops, docs, governance -->
- Existing ADRs to update or supersede: <!-- links under docs/13-adrs/ -->
- Capability matrix row(s) affected: <!-- links / row numbers -->

## Bounded scope (how it would land)

<!-- Describe the smallest "bounded" version of this feature that could
     ship without claiming production readiness. If you don't know, leave
     blank — it's fine to describe a direction. -->

- [ ] In-memory / unit-test scope is enough for the first slice
- [ ] Needs docker-compose stack (Postgres / NATS / MinIO)
- [ ] Needs an ADR
- [ ] Needs OpenAPI / event-contract updates
- [ ] Needs graph-rule tests
- [ ] Needs replay tests (risky apply-path only)

## Non-goals (please be explicit)

<!-- What is explicitly NOT in scope for this request, especially
     anything that could be misread as a production-readiness claim. -->

## Alternatives considered

<!-- Briefly note any alternatives you considered and why you prefer this
     direction. -->

## Acceptance sketch

<!-- A short, testable description of "done" for the bounded slice. -->

## Additional context

<!-- Links, prior art, prior discussions, related issues / PRs. -->
