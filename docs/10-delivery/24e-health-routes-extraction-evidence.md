# P1 Intent API Decomposition — Health Routes Extraction Evidence

> **Status:** BOUNDED DEMO SLICE DONE (local evidence) — 2026-06-07
> **Owner:** BrianNguyen (Backend Lead, solo practitioner)
> **Scope:** Internal evidence for the first bounded P1 Intent API Decomposition demo slice that moved `health_routes.rs` into `routes/health.rs` following the A-09 / S6 pattern.
> **Non-Production Caveat:** This document is an internal planning artifact. It is not a public support document, it does not constitute production-readiness evidence, and it does not claim CI-green status or external sign-off. All external and production evidence gates remain blocked or deferred.

---

## 1. Purpose

The strategic roadmap (`docs/10-delivery/24-strategic-roadmap-and-checklist.md` §5) called for a bounded first decomposition slice to confirm the A-09 / S6 "domain-grouped routes under `crate::routes`" pattern is still safe for leaf modules. The recommended candidate was `health_routes.rs` → `routes/health.rs`. This document records that the slice was delivered, what changed, and the verification gates that pass.

---

## 2. What Changed

### 2.1 Self-Contained `routes/health.rs`

The previously-thin `crates/intent-api/src/routes/health.rs` (8 lines) was rewritten to be the single self-contained health route group. The new module is byte-equivalent in behaviour to the prior top-level `health_routes.rs` plus the prior `add_routes` shim. The following items are now `pub` in `routes::health`:

| Item | Kind | Notes |
|------|------|-------|
| `request_id_middleware` | `pub async fn` | Preserved from `health_routes.rs` lines 31–53; same X-Request-ID extraction / generation logic, same `RequestId(request_id)` extension insertion. |
| `trace_context_middleware` | `pub async fn` | Preserved from `health_routes.rs` lines 71–172; same W3C trace-context extraction / parent-span attachment / response-header injection. |
| `health_handler` | `pub async fn` | `GET /health` — `Json<HealthResponse>` with `status="ok"` and `uptime_seconds` from `state.start_time`. |
| `ready_handler` | `pub async fn` | `GET /ready` — `Json<HealthResponse>` with `status="ready"` and `uptime_seconds=0`. |
| `metrics_handler` | `pub async fn` | `GET /metrics` — Prometheus exporter; uses a `OnceLock<PrometheusHandle>`; `text/plain; version=0.0.4` content-type. |
| `add_routes` | `pub fn` | Wires `GET /health` → `health_handler`, `GET /ready` → `ready_handler`, `GET /metrics` → `metrics_handler` onto a `Router<AppState>`. Now references local symbols instead of `crate::health_routes::*`. |

The `routes/health.rs` module-level doc-comment was updated to record the A-09 / S6 follow-on pattern and the non-production caveats from the original `health_routes.rs` (Phase 3 Batch 2 Slice 2 bounded tracing foundation; not cross-process OTEL propagation).

### 2.2 Reference Rewrite Table

| Old reference | New reference | File |
|----------------|---------------|------|
| `pub mod health_routes;` | (removed) | `crates/intent-api/src/lib.rs` |
| `// Health check routes and middleware have been moved to health_routes.rs` | `// Health check route group and observability middleware live in `routes::health` (P1 demo slice).` | `crates/intent-api/src/lib.rs` |
| `use crate::health_routes;` | (removed; `routes::` is now referenced directly) | `crates/intent-api/src/router.rs` |
| `health_routes::request_id_middleware` | `routes::health::request_id_middleware` | `crates/intent-api/src/router.rs` (axum middleware layer) |
| `health_routes::trace_context_middleware` | `routes::health::trace_context_middleware` | `crates/intent-api/src/router.rs` (axum middleware layer) |
| `crate::health_routes::health_handler` | `health_handler` (local symbol) | `crates/intent-api/src/routes/health.rs` (add_routes) |
| `crate::health_routes::ready_handler` | `ready_handler` (local symbol) | `crates/intent-api/src/routes/health.rs` (add_routes) |
| `crate::health_routes::metrics_handler` | `metrics_handler` (local symbol) | `crates/intent-api/src/routes/health.rs` (add_routes) |
| `health_routes` (Handler Module column for /health, /ready, /metrics) | `routes::health` | `docs/04-api/route-openapi-contract-map.md` (internal) |

### 2.3 Files Deleted

- `crates/intent-api/src/health_routes.rs` — deleted. After the reference rewrites, no remaining `crate::health_routes` references exist; verified by `grep -r health_routes` post-rewrite (remaining matches are: the new `routes/health.rs` doc-comment historical note and the corrected `24-strategic-roadmap-and-checklist.md` text).

---

## 3. Behaviour Preservation

| Observable | Status | Notes |
|------------|--------|-------|
| `GET /health` route path | ✅ preserved | Same path; `health_handler` body unchanged. |
| `GET /ready` route path | ✅ preserved | Same path; `ready_handler` body unchanged. |
| `GET /metrics` route path | ✅ preserved | Same path; `metrics_handler` body unchanged (same `OnceLock<PrometheusHandle>`, same `text/plain; version=0.0.4` content-type). |
| HTTP method for each | ✅ preserved | All three remain `GET`; `add_routes` uses `routing::get(...)` as before. |
| Response body shapes | ✅ preserved | `HealthResponse` JSON shape unchanged for `/health` and `/ready`; Prometheus exposition format unchanged for `/metrics`. |
| Request-id middleware | ✅ preserved | Same X-Request-ID header extraction, same `RequestId` extension insertion, same ordering. |
| Trace-context middleware | ✅ preserved | Same W3C traceparent/tracestate extraction, same span context attachment, same response header injection. |
| Middleware ordering | ✅ preserved | `request_id_middleware` is layered before `trace_context_middleware` (so the trace context sees any extracted parent context). |
| Public exports | ✅ preserved | `routes::health` is a `pub mod` declared in `crates/intent-api/src/routes/mod.rs`; `add_routes` and the middleware fns are `pub`. `lib.rs` re-exports of `HealthResponse` and `RequestId` are unchanged. |
| Tests | ✅ preserved | 442 lib tests pass (no test count delta); all webhook/router/handler tests that touch the `/health`/`/ready`/`/metrics` paths still pass. |

---

## 4. Sequential Verification Gates

| # | Gate | Command | Result |
|---|------|---------|--------|
| 1 | Format | `cargo fmt --all -- --check` | ✅ Pass (no diff) |
| 2 | Type check (target crate) | `cargo check -p intent-api --all-features` | ✅ Pass (55.42s) |
| 3 | Lint (target crate) | `cargo clippy -p intent-api --all-features -- -D warnings` | ✅ Pass (34.47s) |
| 4 | Lib tests (target crate) | `cargo test -p intent-api --lib` | ✅ Pass — 442 passed, 0 failed, 17 ignored, 0 measured |
| 5 | Diff hygiene | `git diff --check` | ✅ Pass (no conflicts) |
| 6 | Workspace check (sanity) | `cargo check --workspace --all-features` | ✅ Pass (1m 08s) |

> **Note on ignored tests:** 17 tests are `#[ignore]`-d in the default run. Those are pre-existing `DATABASE_URL`-gated smoke tests (audit / bundle / graph / outbox / webhook subscription SQLx smoke). They are **not** introduced or modified by this slice; the count is the pre-extraction baseline.

---

## 5. Follow-Up Items (Out of Scope for This Slice)

The following items are **not** part of this bounded demo slice. Each is documented so a future slice owner can pick it up without re-derivation.

1. **`22-phase-4-entry-plan.md` A-09 cross-update.** The roadmap (`24-strategic-roadmap-and-checklist.md` §5.5) recommends updating A-09 to record the continuation slice. The slice handoff explicitly forbids cross-doc edits beyond the roadmap and tracker; this is left to a follow-up A-09 continuation slice.
2. **Next-candidate leaf extraction.** The natural next candidates are the top-level handler modules under `crates/intent-api/src/` that are not yet grouped under `routes/<domain>.rs` (the §5.5 "Next candidate" paragraph enumerates them). The A-09 / S6 pattern (leaf module → `routes/<domain>.rs` plus a single `add_routes` aggregator) is now proven safe by this demo slice.
3. **P2 Benchmark Compile Guard compile-guard evidence.** The roadmap §8 wording was corrected in the same pass (real criterion source files exist in all 4 crates; the remaining bounded work is compile-guard verification, not creating missing files). The `cargo bench --workspace --no-run` evidence run and the per-bench fixture/feature doc note are deferred to a follow-up bounded slice so this P2 item can be claimed closed with evidence.

---

## 6. Local-Bounded Caveats

- This slice is a **code-organization** change only. It does not modify observable HTTP behaviour, route paths, response bodies, headers, middleware ordering, metrics semantics, or test counts.
- This slice is **not** a production-readiness claim. External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.
- The decomposition is a **local-executable** change; it requires no infrastructure, no external services, no user decision, and no production rollout.
- The only Rust module-path difference visible to a future maintainer is the new `routes::health::*` symbol path. Pre-extraction references (in any stale wiki, ADRs, or old notes) to `crate::health_routes::*` will no longer compile if revived; the corrected paths are recorded in §2.2 above.

---

## 7. Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §5 | P1 Intent API Decomposition section; §5.5 action checklist and §5.10 evidence link point to this document. |
| `docs/10-delivery/22-phase-4-entry-plan.md` A-09 | A-09 file-decomposition tracker; the follow-up cross-update from §5.5 is the next slice handoff. |
| `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 | Update log row records this slice and the verification result. |
| `docs/04-api/route-openapi-contract-map.md` (internal) | Route / OpenAPI contract map; `Handler Module` column updated for the three health routes. |
| `crates/intent-api/src/routes/health.rs` | New self-contained health route group module. |
| `crates/intent-api/src/router.rs` | Top-level router; middleware layering rewritten to use `routes::health::*`. |
| `crates/intent-api/src/lib.rs` | Removed `pub mod health_routes;` and the stale "moved" comment. |
| `crates/intent-api/src/health_routes.rs` | Deleted. |

---

## 8. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-07 | BrianNguyen (via authorized assistant fixer) | Initial creation — records the P1 Intent API Decomposition demo slice that moved `health_routes.rs` into `routes/health.rs`: self-contained module, reference-rewrite table, file deletion, behaviour-preservation table, sequential verification gates, follow-up items, local-bounded caveats, and relationship to other documents. No public-doc edits. No production-readiness claim. External gates remain blocked. |
