#!/usr/bin/env bash
# RLS DML Structural Audit (S2)
#
# Verifies structural invariants around RLS-sensitive DML in intent-api handler
# modules. Exits 0 if the current known state matches expected coverage/warnings.
# Exits non-zero only for unexpected invariant failures.
#
# Limitations:
# - Structural audit only; live rls_integration tests remain ground truth.
# - Read-only handlers and in-memory repos are excluded.
# - Webhook gaps are local-known residuals (warnings, not failures).
# - Does not inspect repository implementations outside handler boundary.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
API_SRC="$ROOT/crates/intent-api/src"
SERVICE_SRC="$ROOT/crates/intent-service/src"

PASS=0
WARN=0
FAIL=0

pass() {
    PASS=$((PASS + 1))
    printf "  [PASS] %s\n" "$1"
}

warn() {
    WARN=$((WARN + 1))
    printf "  [WARN] %s\n" "$1"
}

fail() {
    FAIL=$((FAIL + 1))
    printf "  [FAIL] %s\n" "$1"
}

echo "=== RLS DML Structural Audit ==="
echo ""

# ---------------------------------------------------------------------------
# 1. Delegated RLS methods in intent-service
# ---------------------------------------------------------------------------
echo "-- Delegated RLS Methods --"
if grep -q "create_intent_with_rls" "$SERVICE_SRC/intent_service.rs" && \
   grep -q "create_version_with_rls" "$SERVICE_SRC/intent_service.rs"; then
    pass "intent_service.rs contains delegated RLS methods (create_intent_with_rls, create_version_with_rls)"
else
    fail "intent_service.rs missing expected delegated RLS methods"
fi

# ---------------------------------------------------------------------------
# 2. Handler module classification
# ---------------------------------------------------------------------------
echo ""
echo "-- Handler Module Invariants --"

# RLS_WRAPPED: must contain begin_with_tenant or _with_rls
RLS_WRAPPED="
approval_mutation_handlers.rs
batch_handlers.rs
compensation_mutation_handlers.rs
forensic_handlers.rs
graph_handlers.rs
ingest_handlers.rs
intent_mutation_handlers.rs
orchestration_run_handlers.rs
query_handlers.rs
rebase_apply_handlers.rs
replay_handlers.rs
trigger_reapproval_handlers.rs
"

for f in $RLS_WRAPPED; do
    path="$API_SRC/$f"
    if [ ! -f "$path" ]; then
        fail "$f: file not found"
        continue
    fi
    if grep -qE "begin_with_tenant|_with_rls" "$path"; then
        pass "$f: RLS-wrapped (begin_with_tenant or _with_rls present)"
    else
        fail "$f: expected RLS wrapping but none found"
    fi
done

# READONLY: must NOT contain begin_with_tenant or _with_rls
READONLY="
approval_handlers_readonly.rs
compensation_planner_handlers.rs
compensation_query_handlers.rs
diff_handlers.rs
intent_read_handlers.rs
intent_validation_handlers.rs
policy_snapshot_handlers.rs
rebase_preview_handlers.rs
simulation_handlers.rs
"

for f in $READONLY; do
    path="$API_SRC/$f"
    if [ ! -f "$path" ]; then
        fail "$f: file not found"
        continue
    fi
    if grep -qE "begin_with_tenant|_with_rls" "$path"; then
        fail "$f: unexpected RLS pattern in read-only handler"
    else
        pass "$f: read-only (no RLS tx patterns)"
    fi
done

# WEBHOOK_GAP: DML without RLS wrapping — known gap, warning only
WEBHOOK_GAP="
webhook_subscription_handlers.rs
webhook_outbox_dlq_handlers.rs
"

for f in $WEBHOOK_GAP; do
    path="$API_SRC/$f"
    if [ ! -f "$path" ]; then
        fail "$f: file not found"
        continue
    fi
    if grep -qE "begin_with_tenant|_with_rls" "$path"; then
        fail "$f: webhook gap now has RLS wrapping — update audit expectations"
    else
        warn "$f: known webhook gap (no RLS wrapping)"
    fi
done

# ---------------------------------------------------------------------------
# 3. Unclassified handler module detection
# ---------------------------------------------------------------------------
echo ""
echo "-- Unclassified Handler Check --"

for path in "$API_SRC"/*handlers.rs; do
    if [ ! -f "$path" ]; then
        continue
    fi
    f=$(basename "$path")
    # Skip test files
    case "$f" in
        *_tests.rs) continue ;;
    esac

    known=0
    for k in $RLS_WRAPPED $READONLY $WEBHOOK_GAP; do
        if [ "$f" = "$k" ]; then
            known=1
            break
        fi
    done

    if [ "$known" -eq 0 ]; then
        fail "$f: unclassified handler module — add to audit expectations"
    fi
done

# ---------------------------------------------------------------------------
# 4. Webhook SQLx DML warning (non-handler boundary)
# ---------------------------------------------------------------------------
echo ""
echo "-- Webhook SQLx DML Check --"

if [ -f "$API_SRC/webhook_delivery.rs" ]; then
    if grep -q "sqlx::query" "$API_SRC/webhook_delivery.rs"; then
        warn "webhook_delivery.rs: contains sqlx::query (known non-RLS-wrapped webhook SQLx DML)"
    else
        pass "webhook_delivery.rs: no sqlx::query"
    fi
else
    warn "webhook_delivery.rs: not found"
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "========================================"
printf "Summary: %d passed, %d warnings, %d failures\n" "$PASS" "$WARN" "$FAIL"
echo "========================================"

if [ "$FAIL" -gt 0 ]; then
    echo "Audit FAILED: unexpected invariant failures found."
    exit 1
else
    echo "Audit PASSED: known state matches expectations (warnings are documented gaps)."
    exit 0
fi
