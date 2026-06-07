# Panic Alerting Integration Design and Runbook (S7)

> **Status:** DESIGN / LOCAL RUNBOOK COMPLETE + METRIC IMPLEMENTED LOCALLY — Production alerting integration blocked
> **Date:** 2026-05-21 (S7 metric code-complete 2026-06-07)
> **Owner:** BrianNguyen (Backend Lead, solo practitioner)
> **Scope:** S7 — Panic hardening alerting integration design and local validation procedure

---

## 1. Purpose

This document records the **design and local runbook** for panic-event alerting integration. It documents the intended alert path from runtime panic to operator notification, the local validation approach, and the production prerequisites that remain blocked.

> **⚠️ Non-Production Caveat**
>
> This document is a **design and planning artifact only**. No production alerting is configured. No panic metric is currently instrumented. No external alert receiver is wired. All production alerting gates remain blocked.

---

## 2. Current State (Delivered)

| Component | Status | Evidence |
|-----------|--------|----------|
| Panic hook registration | ✅ Delivered | `crates/intent-api/src/panic_hardening.rs` — `init_panic_hook()` registered in `main.rs` before async tasks spawn |
| Sanitized panic logging | ✅ Delivered | `sanitize_panic_payload()` redacts JWT tokens, DB URLs, AWS credentials, Bearer tokens; truncates long payloads |
| Worker panic formatting | ✅ Delivered | `format_join_error()` used by `DlqMetricsWorker`, `DlqReplayWorker`, and `WebhookOutboxWorker` to sanitize join-error panics |
| Unit test coverage | ✅ Delivered | `crates/intent-api/src/panic_hardening_tests.rs` — 9 tests covering sanitization, hook registration, join-error formatting, and `process_panics_total` counter increment (thread-local recorder) |
| Prometheus panic metric | ✅ IMPLEMENTED LOCALLY | `process_panics_total` counter incremented inside `panic_hook()` via `record_panic_event()` in `crates/intent-api/src/panic_hardening.rs`; thread-local test recorder verifies increment in `test_record_panic_event_increments_counter` |
| Prometheus alert rule | ❌ NOT IMPLEMENTED | No panic-specific alert rule defined (see §3.1 for design) |
| Alertmanager receiver routing | ❌ NOT CONFIGURED | Local Alertmanager routes to `alert-receiver:9094/webhook` placeholder only; no external routing |

---

## 3. Intended Alert Path (Design)

```text
Runtime panic
  └── std::panic::set_hook (panic_hardening.rs)
        ├── eprintln! sanitized payload (current — observable in logs)
        └── process_panics_total counter ++ (IMPLEMENTED LOCALLY; production alerting blocked)
              └── Prometheus scrape
                    └── Alert rule: ProcessPanicDetected
                          └── Alertmanager route (severity: critical)
                                └── Receiver: PagerDuty / Slack / email (production)
                                      └── On-call operator responds per RB15
```

### 3.1 Planned Prometheus Alert Rule (Design-Only)

The following rule is **documented as the intended design** but is **not yet deployed** because the underlying metric does not exist.

```yaml
# Design-only — prerequisite: process_panics_total counter must be instrumented
# in panic_hardening.rs before this rule can be loaded.
- name: intent_api_panic_hardening
  interval: 15s
  rules:
    - alert: ProcessPanicDetected
      expr: increase(process_panics_total[1m]) > 0
      for: 0s
      labels:
        severity: critical
        slo: reliability
      annotations:
        summary: "Process panic detected on {{ $labels.instance }}"
        description: "A panic occurred in the intent-api process. Location: {{ $labels.location }}. See logs for sanitized payload."
        runbook_url: "docs/09-operations/05-runbooks.md#RB15-Process-Panic-Detected"
        scope: "design-only — metric not instrumented; production alerting requires SRE sign-off and receiver configuration"
```

**Prerequisite to activate:** Instrument `process_panics_total` counter in `panic_hardening.rs` (increment inside `panic_hook` before logging). **Implemented locally (2026-06-07):** the counter is now incremented by `record_panic_event()` inside `panic_hook()` in `crates/intent-api/src/panic_hardening.rs`. The rule itself remains design-only — production activation still requires staging, external alert receivers, and SRE sign-off.

### 3.2 Alertmanager Routing Design

| Severity | Route | Production Receiver (Blocked) |
|----------|-------|------------------------------|
| `critical` | `ProcessPanicDetected` | PagerDuty/OpsGenie on-call rotation (requires external SRE config) |

Current local Alertmanager (`infrastructure/local/alertmanager/alertmanager.yml`) routes all alerts to `http://alert-receiver:9094/webhook`, which is a Python helper script that prints JSON to stdout. This is **local-dev/manual-only** and does not route to external systems.

---

## 4. Local Validation Approach

The `process_panics_total` counter is now implemented locally. Full end-to-end alert firing still cannot be validated until staging, receivers, and SRE sign-off are in place. The following bounded local validations are available:

### 4.1 Hook Behavior Validation (Available Now)

```bash
# Run panic hardening unit tests
cargo test -p intent-api --lib panic_hardening -- --nocapture
```

**Expected:** 10 tests pass, verifying:
- JWT token redaction in panic payload
- Database URL redaction
- AWS credential redaction
- Bearer token redaction
- Long payload truncation
- No-op on clean strings
- Hook registration does not panic
- `format_join_error` sanitizes panic messages from tokio task join errors (panic + abort paths)
- `record_panic_event` increments the `process_panics_total` counter (via a
  thread-local test recorder to avoid global metrics-recorder conflicts)

### 4.2 Alert Rule Syntax Validation (Available Now)

If the design-only rule from §3.1 is copied into a file:

```bash
# Validate rule syntax with promtool (requires local Prometheus installation)
promtool check rules /tmp/panic_alert_design.yml
```

**Expected:** Syntax passes (expression is valid PromQL). The rule will **not evaluate to firing** because no Alertmanager is wired to a Prometheus that scrapes a process exposing `process_panics_total`; even though the counter is now instrumented locally, end-to-end production alerting still requires staging, receivers, and SRE sign-off.

### 4.3 Alertmanager Routing Validation (Available Now)

```bash
# 1. Start local observability profile
docker compose -f infrastructure/local/docker-compose.yml --profile observability up -d alert-receiver alertmanager

# 2. Post a manual test alert mimicking the panic alert shape
curl -X POST http://localhost:9093/api/v1/alerts \
  -H "Content-Type: application/json" \
  -d '[{"labels":{"alertname":"ProcessPanicDetected","severity":"critical","slo":"reliability","instance":"localhost:8080"},"annotations":{"summary":"Design validation: panic alert routing","description":"This is a manual test of the Alertmanager routing path for panic alerts."}}]'

# 3. Inspect alert-receiver logs for routed payload
docker compose -f infrastructure/local/docker-compose.yml --profile observability logs alert-receiver

# 4. Clean up
docker compose -f infrastructure/local/docker-compose.yml --profile observability stop alert-receiver alertmanager
docker compose -f infrastructure/local/docker-compose.yml --profile observability rm -f alert-receiver alertmanager
```

**Expected:** Alert JSON appears in `alert-receiver` container logs. This validates the local Alertmanager → webhook receiver path but does **not** validate:
- Prometheus rule evaluation (no metric)
- External receiver routing (Slack/PagerDuty)
- Production Alertmanager topology

### 4.4 Load-Test Panic Observability (Partial)

During local load tests, no panics have been observed (L1-L3 sustained 90s smoke: 4505/4505 success, 0% error). If a panic were to occur during load testing, it would be visible in:
- Application stderr logs (via `eprintln!` in panic hook)
- Container logs (`docker logs intent-rebase-api`)
- The `process_panics_total` counter (locally exposed via `/metrics` once the
  `metrics-exporter-prometheus` recorder is installed at startup)
- But **not** in Prometheus/Alertmanager (no production scraper/receivers wired)

---

## 5. Response / Runbook Procedure

See **RB15 — Process Panic Detected** in [`docs/09-operations/05-runbooks.md`](05-runbooks.md#RB15-Process-Panic-Detected) for the operator response procedure.

Summary:
1. **Acknowledge** — Confirm alert is genuine (check logs for `PANIC:` prefix)
2. **Contain** — If panic is recurring, restart affected pod/container; panics do not crash the tokio runtime but may leave tasks in failed state
3. **Investigate** — Collect sanitized panic payload, file location, thread name from logs
4. **Escalate** — If panic indicates data corruption or security issue, escalate to backend lead immediately
5. **Remediate** — Fix root cause; do not rely on panic hook as error-handling mechanism
6. **Verify** — After fix, run `cargo test --workspace --lib --all-features` and load tests to confirm stability

---

## 6. Blocked Production Prerequisites

The following prerequisites must be satisfied before panic alerting can be considered production-ready. All are currently blocked.

| Prerequisite | Why Required | Current State | Blocker |
|--------------|--------------|---------------|---------|
| `process_panics_total` counter metric | Prometheus alert rule requires a metric to evaluate | ✅ IMPLEMENTED LOCALLY (2026-06-07) in `crates/intent-api/src/panic_hardening.rs::record_panic_event()`; unit test verifies increment via thread-local recorder | None locally; production wiring still requires scraper, receivers, SRE sign-off |
| Staging environment | Alert rule must be validated against realistic traffic and fault injection | No staging infra exists | A-05 (Production Infrastructure) blocked |
| External alert receivers (PagerDuty/Slack/email) | On-call operator must be notified when critical alert fires | Local webhook receiver only | A-03 (External SRE Sign-Off) blocked |
| Alertmanager production topology | Routing, grouping, inhibition, and silencing must be configured for production | Single local instance with placeholder routes | A-05 + A-03 blocked |
| SRE sign-off | Alert threshold, receiver configuration, and runbook must be reviewed by external SRE | Solo self-review only | A-03 blocked |
| 30min sustained load test with fault injection | Panic alert must not fire under normal load; must fire under injected fault conditions | L4 blocked (no staging infra) | A-06 (Load Testing L3-L5) blocked |

---

## 7. Forbidden Claims

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `Panic alerts fire in production` | `Panic alerting is design-only; the process_panics_total counter is implemented locally but no receiver is configured and no production scraper is wired` |
| `Panic alerts route to PagerDuty/Slack` | `Local Alertmanager routes to a placeholder webhook receiver; external routing requires SRE config` |
| `Panic hardening is production-ready` | `Panic hook, sanitized logging, and process_panics_total counter are delivered locally; alerting integration is design-only` |
| `Prometheus panic alert rule is active` | `Alert rule is documented in design only; the metric prerequisite is now implemented locally but the rule itself is not deployed` |

---

## 8. Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/09-operations/05-runbooks.md` (RB15) | Operator response procedure for panic alerts |
| `docs/09-operations/03-observability.md` | General observability stack description and local-only caveats |
| `docs/09-operations/09-observability-evidence-checklist.md` | Local validation templates for Prometheus/Alertmanager |
| `docs/10-delivery/22-phase-4-entry-plan.md` (A-08) | Phase 4 panic hardening tracker; this doc is the S7 deliverable |
| `docs/10-delivery/23-project-assessment-and-execution-tracker.md` (S7) | Execution tracker status for S7 |
| `crates/intent-api/src/panic_hardening.rs` | Existing panic hook implementation (no metric) |
| `infrastructure/local/prometheus/rules/intent_api_alerts.yml` | Existing alert rules (no panic rule — design documented here) |
| `infrastructure/local/alertmanager/alertmanager.yml` | Local Alertmanager config (placeholder receivers only) |

---

## 9. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-05-21 | BrianNguyen (via authorized assistant fixer) | Initial S7 design/runbook — current state, intended alert path, planned Prometheus rule (design-only), local validation approach, RB15 response procedure, blocked production prerequisites, forbidden claims, and cross-references. No production-readiness claim. No runtime code changes. |
| 2026-06-07 | BrianNguyen (via authorized assistant fixer) | S7 metric code-complete locally — added `process_panics_total` counter increment via `record_panic_event()` inside `panic_hook()` in `crates/intent-api/src/panic_hardening.rs`; added `test_record_panic_event_increments_counter` bounded test in `panic_hardening_tests.rs` using a thread-local test recorder to avoid global metrics-recorder conflicts. Design/runbook doc updated: §2 current state (9-test unit-test row, metric row flipped to IMPLEMENTED LOCALLY), §3 intended alert path, §3.1 prerequisite note, §4.1 test count to 10, §4.2 expected evaluation, §4.4 load-test observability, §6 blocked prerequisites (metric row flipped to IMPLEMENTED LOCALLY), and §7 forbidden-claim wording all refreshed. Production alerting still blocked on staging, external receivers, and SRE sign-off. No production-readiness claim. |
