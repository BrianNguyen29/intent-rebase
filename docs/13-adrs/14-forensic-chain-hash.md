# ADR-14 — Forensic Chain-Hash Algorithm and Linking Protocol

## Status

**Accepted — Local algorithm implemented** — Pure chain-hash module (`chain_hash.rs`) delivers tamper-evident linking between bundles. Object Lock, S3 retention enforcement, and production deployment remain Phase 4+ deferred scope.

## Context

Forensic bundles must be tamper-evident: any modification to a bundle or its predecessors must be detectable. The existing `bundle_hasher.rs` provides per-section integrity hashes (SHA-256 of intent versions, artifacts, approvals, audit events, policy snapshots). What is missing is a **chain** that links bundles together so that tampering with an older bundle invalidates all subsequent bundles.

### Design Question

How should bundles be cryptographically linked into a tamper-evident chain?

- **Option A: Linear chain hash** — Each bundle's chain hash incorporates the previous bundle's chain hash: `H(prev_chain_hash || manifest_hash)`. Simple, deterministic, and easy to verify.
- **Option B: Merkle tree** — Bundles are leaves in a Merkle tree; the root is signed/witnessed. More scalable for parallel verification but overkill for current bundle volume.
- **Option C: Timestamped signed hash** — Each bundle hash is signed by a trusted timestamp authority. Requires external infrastructure and key management.

## Decision

**Option A (linear chain hash) is accepted and locally implemented.**

Rationale:
1. **Simplicity** — A single SHA-256 computation per bundle is trivial to implement, test, and audit.
2. **No external dependencies** — No timestamp authority, no key management, no blockchain. Pure local algorithm.
3. **Sufficient for current volume** — Forensic bundles are generated infrequently (incident-driven or compliance-scheduled). Linear chain is adequate.
4. **Natural with existing hashes** — The per-section `manifest_hash` already exists in `BundleIntegrity`. Chain hash simply wraps it.

### Algorithm

```
manifest_hash  = SHA256(canonical_json(bundle_manifest))
chain_hash     = if previous_bundle_hash is Some(prev):
                      SHA256(prev || manifest_hash)
                 else:
                      manifest_hash
```

Verification:
- Given an ordered sequence of bundles and their stored `previous_bundle_hash` values, recompute every `chain_hash`.
- If any recomputed `previous_bundle_hash` does not match the stored value, the chain is broken.
- Full tamper detection also requires comparing the final chain hash against an externally witnessed value (Object Lock, signed receipt) — Phase 4+ scope.

## Consequences

### Positive
- Pure local implementation with no infra dependencies
- Deterministic and testable
- Reuses existing SHA-256 infrastructure
- `BundleIntegrity` extended with `previous_bundle_hash: Option<String>`

### Negative
- Linear chain verification is O(n) in the number of bundles. For very long chains this could be slow.
- Without an external witness (signed final hash, Object Lock), the chain can be regenerated after tampering. The algorithm detects inconsistency but not absolute truth.
- No automatic enforcement; verification is a manual/scheduled operation until Phase 4+.

### Neutral
- `chain_hash.rs` is a standalone module in `forensic-service`. It does not depend on S3, PostgreSQL, or async runtime.
- The `BundleIntegrity.chain_verified` flag remains `false` by default; a future slice can set it after running `verify_chain`.

## Implementation Notes

**Implemented (bounded):**
- `crates/forensic-service/src/chain_hash.rs` provides:
  - `ChainHashLink` struct (`previous_bundle_hash`, `current_bundle_hash`, `chain_hash`, `linked_at`)
  - `compute_chain_hash(previous_bundle_hash, manifest_json) -> Result<ChainHashLink, ...>`
  - `verify_chain(links) -> Result<(), ChainVerificationFailure>`
- `BundleIntegrity` extended with `previous_bundle_hash: Option<String>`
- Comprehensive unit tests for genesis bundle, linked bundle, tamper detection, and chain verification

**Not implemented (out of scope):**
- S3 Object Lock deployment and validation
- Retention policy enforcement
- Automatic scheduled chain verification
- Signed final hash or external timestamp authority
- Production security review and infrastructure gates (A-05, A-04)

## Evidence

- Chain hash module: `crates/forensic-service/src/chain_hash.rs`
- Bundle integrity model: `crates/forensic-service/src/bundle.rs` (`BundleIntegrity.previous_bundle_hash`)
- Module registration: `crates/forensic-service/src/lib.rs`

## Related ADRs

- ADR-02 (Data Plane): Storage architecture
- ADR-12 (Workflow Migration): Phase 4 forensic replay design

## Review History

| Date | Reviewer | Notes |
|------|----------|-------|
| 2026-06-10 | (fixer) | ADR created; linear chain-hash decision recorded; pure local algorithm and tests delivered |

---

**Next Step**: Integrate chain-hash computation into bundle generation so that each new bundle stores `previous_bundle_hash`. Wire external witness (Object Lock, signed receipt) in Phase 4+.
