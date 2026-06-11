# Agent Implementation Guide

## Purpose
Give AI agents a clear map to implement without ambiguity.

## Agent working rules
1. Do not change intent schema without updating the ADR.
2. Every API change must update OpenAPI and the event contract.
3. Every graph rule change must include tests.
4. Every risky apply-path change must have replay tests.
5. Do not implement S3/S4 side-effect auto-compensation without explicit approval.

## Recommended workstreams

### Stream A — Core Data and APIs
- schema migrations
- intent CRUD
- diff endpoints
- rebase endpoints

### Stream B — Control Logic
- semantic diff rules
- graph propagation engine
- rebase planner

### Stream C — Runtime Integration
- adapter capability contract
- primary adapter
- checkpoint mapping
- apply pipeline

### Stream D — Console
- intent detail
- rebase preview
- workflow timeline
- approvals/stale indicators

### Stream E — Security and Audit
- authz
- audit append
- export
- permissions matrix

## Definition of done for each task
- code
- tests
- docs
- metrics/logging
- security notes
- rollback note
