# CI/CD

> **Last Updated:** April 2026

## Current CI State (Actual)

> **Remote CI Status:** 🔴 `startup_failure` — GitHub Actions reports startup_failure before jobs are created (run 25273892755 after commit 42cdbe2). Local canonical gates pass; remote CI is not passing.

### Actual Workflows

| Workflow | File | Status | Description |
|----------|------|--------|-------------|
| **CI** | `.github/workflows/ci.yml` | 🔴 BLOCKED (startup_failure) | Full Rust workspace: fmt, clippy, check, test, openapi-validate, build, bench, test-db, docker-build |
| **Smoke** | `.github/workflows/smoke.yml` | ⚠️ STUB | `echo "hello"` only — not a real smoke test |

### Actual CI Jobs (ci.yml)

| Job | Status | Command |
|-----|--------|---------|
| fmt | ✅ Runs | `cargo fmt --all -- --check` |
| clippy | ✅ Runs | `cargo clippy --workspace --all-targets -- -D warnings` |
| check | ✅ Runs | `cargo check --workspace` |
| test | ✅ Runs | `cargo test --workspace` |
| openapi-validate | ✅ Runs | `npx spectral lint docs/04-api/openapi.yaml` |
| build | ✅ Runs | `cargo build --workspace --release` |
| bench | ✅ Runs | `cargo bench -p rebase-engine` |
| test-db | ✅ Runs | `cargo test -p intent-service --test migration_integration -- --ignored` |
| docker-build | ✅ Runs | `docker build-push-action` |

**Local Gate Equivalents:** All jobs run locally via their respective `cargo` commands.

**No overclaim:** Remote CI reports `startup_failure`. Do not claim CI passes.

---

## Aspirational Pipeline (Not Yet Implemented)

The following describes the **target production pipeline** that is NOT yet implemented:

```
1. lint / format            ← ci.yml has this
2. unit tests               ← ci.yml has this (cargo test --workspace)
3. contract tests           ← NOT implemented
4. integration tests         ← ci.yml has test-db (migration only); broader integration tests not implemented
5. replay tests              ← NOT implemented (requires full Phase 3 exit)
6. security scans            ← NOT implemented
7. image signing             ← NOT implemented
8. deploy to preview/staging ← NOT implemented (docker-build creates image but no deploy job)
9. smoke tests               ← smoke.yml is a stub (echo hello); not a real smoke test
10. controlled production rollout ← NOT implemented
```

### Missing Pipeline Components

| Component | Status | Notes |
|-----------|--------|-------|
| Contract tests | 🔴 Not implemented | Requires Phase 3 exit |
| Replay tests | 🔴 Not implemented | Requires Phase 3 exit |
| Security scans | 🔴 Not implemented | Requires security tooling setup |
| Image signing | 🔴 Not implemented | Requires cosign/signing infrastructure |
| Preview/staging deploy | 🔴 Not implemented | Requires deployment infrastructure |
| Real smoke tests | 🔴 Not implemented | smoke.yml is stub; requires service endpoint tests |
| Production rollout | 🔴 Not implemented | Requires production infra |

---

## Special Requirements for IRE

> **Note:** These are design/operational requirements that exist in the architecture but are not yet enforced by CI.

- [ ] rule pack versioning — artifact in docs/architecture; not CI-enforced
- [ ] diff classifier versioning — artifact in docs/architecture; not CI-enforced
- [ ] replay tests against historical rebase scenarios — NOT implemented
- [ ] adapter compatibility tests — NOT implemented

---

## Release Strategies

> **Note:** These are target strategies documented in design; no automated release pipeline exists.

- [ ] canary — NOT implemented (requires deployment infrastructure)
- [ ] blue/green where possible — NOT implemented (requires deployment infrastructure)
- [ ] feature flags for classifier behavior — implementation exists; not yet in CI
- [ ] tenant allowlists for risky features — implementation exists; not yet in CI

---

## CI/CD Truths

1. **Remote CI is broken** — `startup_failure` blocks all remote gates
2. **Smoke workflow is a stub** — `echo hello` is not a real smoke test
3. **The aspirational pipeline (steps 1–10) is not implemented** — only steps 1–2 are covered by current CI; migration-only integration testing and docker image build exist as bounded partials, but deployment/security/signing/real smoke/production rollout are not implemented
4. **Local gates pass** — `cargo fmt`, `cargo clippy`, `cargo check`, `cargo test`, `cargo bench` all pass locally
5. **No deployment pipeline** — docker-build creates an image but nothing deploys it
6. **No security scans** — no Trivy, Grype, or similar in CI

---

## Related Documents

- [Current Status](../10-delivery/00-current-status.md)
- [Production Readiness Backlog](../10-delivery/17-production-readiness-backlog.md)
- [Solo Ops Evidence Plan](../10-delivery/16-solo-ops-evidence-plan.md)
