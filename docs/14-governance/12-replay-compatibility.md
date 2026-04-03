# 12 — Replay Compatibility

**Status:** Proposed  
**Phase:** Phase 2+  
**Owner:** Platform Team

---

## Mục đích

Define guarantees and requirements for replaying intent execution, ensuring:
- **Reproducibility** — same intent version produces same result
- **Compatibility** — replay works across system upgrades
- **Auditability** — replay operations are logged and verifiable

---

## Replay Types

### Checkpoint Replay

```
Replay intent execution from a specific checkpoint:
1. Load checkpoint state
2. Resume from checkpoint
3. Apply any pending rebase directives
4. Continue execution to completion
```

### Intent Version Replay

```
Replay with different intent version:
1. Load new intent version
2. Map to appropriate checkpoint
3. Apply intent changes to execution context
4. Continue execution
```

### Full Replay

```
Complete re-execution from initial state:
1. Load initial checkpoint (or empty state)
2. Load intent version
3. Execute from beginning
4. Compare output to original
```

---

## Compatibility Requirements

### Version Compatibility Matrix

| System Version | Intent v1 | Intent v2 | Intent v3 | Intent v4 |
|----------------|-----------|-----------|-----------|-----------|
| v1.0 | ✓ | ✗ | ✗ | ✗ |
| v1.1 | ✓ | ✓ | ✗ | ✗ |
| v2.0 | ✓ | ✓ | ✓ | ✗ |
| v2.1 | ✓ | ✓ | ✓ | ✓ |

### Compatibility Guarantees

1. **Forward compatibility** — New system versions can replay old intent versions
2. **Backward compatibility** — Old system versions cannot replay new intent versions (graceful error)
3. **Checkpoint compatibility** — Checkpoints are versioned and include migration path

---

## Replay API

```yaml
POST /api/v1/intents/{id}/replay:
  description: Replay intent execution
  body:
    {
      "intent_version": int,  # optional, defaults to current
      "checkpoint_id": "uuid",  # optional, specific checkpoint
      "replay_type": "checkpoint | version | full",
      "options": {
        "dry_run": boolean,
        "compare_output": boolean,
        "capture_diffs": boolean
      }
    }
  response:
    {
      "replay_id": "uuid",
      "status": "started",
      "estimated_completion": "ISO8601"
    }

GET /api/v1/replay/{replay_id}:
  response:
    {
      "replay_id": "uuid",
      "status": "running | completed | failed",
      "started_at": "ISO8601",
      "completed_at": "ISO8601",
      "result": {
        "output_artifacts": ["uuid", ...],
        "output_diff": {...},  # if compare_output enabled
        "compatibility_verified": boolean
      },
      "errors": [...]
    }
```

---

## Replay Safety

### Non-Production Requirement

```
Replay MUST run in isolated, non-production environment:
- Separate replay namespace/tenant
- No impact on live system
- No artifact modification in production
```

### Sandbox Mode

```rust
async fn replay_intent(
    intent_id: Uuid,
    version: Option<i32>,
    env: ReplayEnvironment,
) -> Result<ReplayResult> {
    match env {
        ReplayEnvironment::Production => {
            return Err(ReplayError::ProductionReplayForbidden);
        }
        ReplayEnvironment::Sandbox => {
            // Allowed, with full audit logging
            info!("Replay initiated in sandbox", intent_id, version);
        }
    }
}
```

---

## Replay Verification

### Output Comparison

```rust
struct ReplayVerifier {
    async fn verify(&self, original: &Artifact, replayed: &Artifact) -> bool {
        // Content hash must match
        if original.hash != replayed.hash {
            return false;
        }
        
        // Execution path must be equivalent (semantic comparison)
        let path_similarity = self.compare_execution_paths(original, replayed);
        if path_similarity < 0.95 {
            return false;
        }
        
        true
    }
}
```

### Compatibility Report

```json
{
  "replay_id": "uuid",
  "compatibility_verified": true,
  "original_version": "v1.2.0",
  "replayed_version": "v1.3.0",
  "differences": [
    {
      "aspect": "execution_time",
      "original": "120s",
      "replayed": "118s",
      "within_threshold": true
    },
    {
      "aspect": "output_hash",
      "original": "sha256:abc123",
      "replayed": "sha256:abc123",
      "match": true
    }
  ],
  "warnings": [],
  "passed": true
}
```

---

## Audit & Forensics

### Replay Audit Events

| Event | Purpose |
|-------|---------|
| `replay.initiated` | Replay started |
| `replay.checkpoint_loaded` | Checkpoint state loaded |
| `replay.intent_version_loaded` | Intent version loaded |
| `replay.step_completed` | Replay step completed |
| `replay.completed` | Replay finished |
| `replay.failed` | Replay failed |
| `replay.output_compared` | Output comparison result |

### Forensics Integration

Replay results included in forensic bundles:
- Replay configuration
- Execution trace
- Output comparison
- Any discrepancies found

---

## Related Documents

- [02 — Provenance Specification](./02-provenance-spec.md)
- [10 — Forensic Bundle](./10-forensic-bundle.md)
- [11 — Incident Freeze](./11-incident-freeze.md)