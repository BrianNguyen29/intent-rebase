# 09 — Data Handling & Redaction

**Status:** Proposed  
**Phase:** Phase 1+  
**Owner:** Security Team

---

## Mục đích

Define how PII and sensitive data are handled throughout Intent Rebase Engine, including:
- Data classification
- Redaction rules
- Privacy-preserving logging
- Data retention and deletion

---

## Data Classification

### Classification Levels

| Level | Description | Examples | Handling |
|-------|-------------|----------|----------|
| **PII** | Personally Identifiable Information | Email, name, phone, IP address | Encrypted, redacted in logs |
| **SPI** | Sensitive Personal Information | Financial data, health data, credentials | Encrypted, never in logs |
| **CI** | Confidential Information | Business data, trade secrets | Encrypted at rest |
| **PI** | Public Information | Intent content, artifact names | Standard handling |
| **Audit Metadata** | System-generated metadata | Timestamps, tenant_id, operation type | Standard handling |

---

## Redaction Rules

### Log Redaction

```rust
struct RedactionConfig {
    // Fields to redact in logs
    redact_fields: Vec<&'static str> = vec![
        "email",
        "phone",
        "ip_address",
        "password",
        "credit_card",
        "ssn",
    ],
    
    // Replacement pattern
    redaction_marker: &'static str = "[REDACTED]",
}

impl LogRedactor {
    fn redact(&self, value: &str) -> String {
        // Check if value matches any PII pattern
        if self.is_pii(value) {
            return self.redaction_marker.to_string();
        }
        value.to_string()
    }
}
```

### Structured Log Example

```json
{
  "timestamp": "2025-04-03T12:00:00Z",
  "level": "INFO",
  "trace_id": "abc123",
  "tenant_id": "tenant-uuid",
  "actor_id": "user-uuid",
  "action": "intent.created",
  "target_id": "intent-uuid",
  "metadata": {
    "intent_name": "Update customer data retention",
    "risk_level": "medium",
    "actor_email": "[REDACTED]",
    "actor_ip": "[REDACTED]"
  }
}
```

### PII Detection Patterns

```rust
const PII_PATTERNS: &[(&str, &str)] = &[
    (r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b", "email"),
    (r"\b\d{3}[-.\s]?\d{3}[-.\s]?\d{4}\b", "phone"),
    (r"\b\d{3}[-.\s]?\d{2}[-.\s]?\d{4}\b", "ssn"),
    (r"\b\d{16}\b", "credit_card"),
    (r"\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}\b", "ip_address"),
];
```

---

## Privacy-Preserving Queries

### Audit Event Queries

```sql
-- Audit queries should never return raw PII
-- Instead, return references

-- Instead of:
SELECT actor_email, action FROM audit_events; -- BAD

-- Use:
SELECT actor_id, action FROM audit_events; -- GOOD
```

### Graph Node Queries

```sql
-- Node content may contain PII
-- Store content hash, not raw content
-- For display: use content_hash reference, not raw data

CREATE TABLE graph_nodes (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL,
  content_hash TEXT NOT NULL,  -- SHA256 of actual content
  content_type TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL
);

-- Raw content stored in S3 with encryption, not in DB
-- Access requires separate authorization
```

---

## Data Minimization

### Collection Principles

1. **Only collect what's necessary** — don't store full intent content if hash suffices
2. **Don't retain raw PII** — store encrypted, reference by ID
3. **Anonymize in metrics** — use tenant_id, not user identifiers
4. **Aggregate for analytics** — don't expose individual-level data

### Example: Analytics

```json
// Instead of individual user actions:
{
  "user_email": "user@example.com",
  "actions": 150,
  "intents_created": 12
}

// Use aggregated:
{
  "tenant_id": "tenant-uuid",
  "active_users": 150,
  "intents_created_total": 1200,
  "avg_actions_per_user": 8
}
```

---

## Data Retention & Deletion

### Deletion Workflow

```
1. Delete request received (user, compliance, legal)
2. Verify deletion authorization
3. Identify all data copies (DB, S3, backups, logs)
4. Execute deletion in priority order:
   a. Production DB (soft delete first, then hard delete)
   b. S3 objects (move to quarantine, then delete)
   c. Backup data (handled by backup rotation)
   d. Logs (handled by log retention policy)
5. Verify deletion (sample check)
6. Issue deletion certificate
```

### Deletion API

```yaml
POST /api/v1/data-deletion/request:
  description: Request data deletion for a user/tenant
  body:
    {
      "type": "user | tenant | intent",
      "id": "uuid",
      "reason": "string",
      "authorized_by": "uuid"
    }
  response:
    {
      "request_id": "uuid",
      "status": "pending | processing | completed | failed",
      "estimated_completion": "ISO8601"
    }
```

---

## Encryption Requirements

| Data State | Requirement |
|------------|-------------|
| At rest (Postgres) | AES-256 encryption (AWS RDS / Cloud SQL managed) |
| At rest (S3) | AES-256 + S3 SSE-KMS |
| In transit | TLS 1.2+ (all connections) |
| Backups | Same as production data |
| Logs | No encryption (handled by access control) |

---

## Compliance Mapping

| Regulation | Requirement | Implementation |
|------------|-------------|----------------|
| GDPR | Right to deletion | Deletion workflow + verification |
| GDPR | Data minimization | Collection principles + anonymization |
| GDPR | Breach notification | Alert on data access anomalies |
| CCPA | Data disclosure control | Redaction + access logging |
| HIPAA | PHI protection | SPI classification + encryption |