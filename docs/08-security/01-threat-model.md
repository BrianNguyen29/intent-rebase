# Threat Model

## Assets
- intent versions
- approvals
- artifact provenance
- side effect ledger
- audit events
- forensic exports
- tenant secrets / tokens

## Adversaries
- external attacker spoofing source changes
- malicious insider changing intent to bypass controls
- compromised runtime adapter
- confused deputy through stale approvals
- tenant breakout via graph queries
- replay/export misuse

## Major threats

### T1. Intent spoofing
Attacker sends fake webhook/spec update.

Mitigations:
- signed webhooks
- source trust registry
- actor binding
- idempotency + anti-replay

### T2. Approval confusion
Approval issued under old scope but still used for new intent.

Mitigations:
- approval scope hashing
- policy snapshot binding
- preflight approval revalidation

### T3. Cross-tenant leakage
Graph traversal or replay export leaks other tenant data.

Mitigations:
- tenant isolation in data model
- row-level security
- object store bucket prefix isolation
- export signing and access TTL

### T4. Adapter forgery
Adapter reports wrong checkpoint or applies wrong rebase plan.

Mitigations:
- signed adapter attestations
- capability registry
- contract tests
- state hash verification

### T5. Audit tampering
Actor deletes or modifies incident timeline.

Mitigations:
- append-only audit log
- WORM/immutable retention by tier
- external log sink optional

### T6. Prompt/policy injection via source refs
Spec/ticket contains content that skews diff or rebase classification.

Mitigations:
- source trust tiers
- content sanitization by source type
- low-confidence route to manual review

---

## Data Residency (Bounded Verification/Planning)

**Current state (truthful):**
- Single-region deployment today (no multi-region routing implemented)
- `tenant.region` metadata field exists on Tenant model
- Target-region tagging is recorded but **no enforcement or routing exists**
- Enforcement and routing to tenant-assigned regions remains **future phase scope (Phase 4+)**

**What this means:**
- Tenant data is not currently segmented by region at the storage or API layer
- Bundle generation and storage use the locally configured S3/MinIO endpoint
- Cross-region data residency guarantees cannot be claimed at this time
- Compliance evidence (e.g., GDPR Art. 30 records of processing) should note single-region current state
