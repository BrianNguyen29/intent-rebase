# Test Strategy

> **Note:** Module-level documentation (`//!` headers) and handler module doc-comment completeness are tracked as P2 deferred work (see `docs/10-delivery/20-project-completion-roadmap.md`). They are NOT required for current P0 quality gates.

## Test pyramid

### Unit tests
- diff rules
- graph propagation rules
- rebase classifier
- approval revalidation logic
- side effect class mapping

### Integration tests
- API + DB
- event flows
- adapter contracts
- object store + metadata consistency

### Scenario tests
- coding agent rebase cases
- support workflow policy change
- research workflow budget change
- deployment freeze case

### Replay tests
- historical event streams replay under new code/rules
- validate compatibility and deterministic control decisions

### Chaos / resilience
- queue failures
- adapter partial outages
- audit sink degradation
- duplicate webhooks

## Quality gates
- contract tests pass
- replay tests pass before prod deploys
- no critical security findings
- operator workflows manually validated

## Deferred audits (P2)
- Module-level documentation completeness (`//!` headers for all handler modules)
- Dev-experience verification (`verify-fast.sh` benchmark integration, optional `justfile`)
- Cross-link consistency between test strategy and delivery docs
