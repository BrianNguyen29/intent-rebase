# 07 — Authorization Matrix

**Status:** Proposed  
**Phase:** Phase 1+  
**Owner:** Security Team

---

## Mục đích

Defines role-based access control (RBAC) for Intent Rebase Engine, specifying who can perform which actions on which resources under which conditions.

---

## Role Definitions

### System Roles

| Role | Description | Scope |
|------|-------------|-------|
| `system:admin` | Full system access | All tenants |
| `system:security` | Security operations | All tenants |
| `system:auditor` | Read-only audit access | All tenants |

### Tenant Roles

| Role | Description | Scope |
|------|-------------|-------|
| `tenant:owner` | Full tenant administration | Own tenant only |
| `tenant:admin` | Tenant management | Own tenant only |
| `tenant:developer` | Intent and artifact management | Own tenant only |
| `tenant:operator` | Day-to-day operations | Own tenant only |
| `tenant:viewer` | Read-only access | Own tenant only |
| `tenant:security-reviewer` | Approval and security review | Own tenant only |
| `tenant:auditor` | Read-only audit and compliance | Own tenant only |

### Special Roles

| Role | Description | Scope |
|------|-------------|-------|
| `service:intent-service` | Intent service account | Own tenant only |
| `service:rebase-engine` | Rebase engine service account | Own tenant only |
| `service:audit-service` | Audit service account | Own tenant only |

---

## Resource Types

| Resource | Description |
|----------|-------------|
| `Intent` | Intent documents and versions |
| `Artifact` | Produced artifacts |
| `Approval` | Approval workflow instances |
| `PolicySnapshot` | Policy snapshots |
| `RulePack` | Rule pack configurations |
| `AuditEvent` | Audit log entries |
| `GraphNode` | Dependency graph nodes |
| `GraphEdge` | Dependency graph edges |
| `Tenant` | Tenant configuration |
| `User` | User accounts |

---

## Authorization Matrix

### Intent Operations

| Action | Admin | Developer | Operator | Viewer | Security Reviewer |
|--------|-------|-----------|----------|--------|-------------------|
| `intent:create` | ✓ | ✓ | ✗ | ✗ | ✗ |
| `intent:read` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `intent:update` | ✓ | ✓ | ✗ | ✗ | ✗ |
| `intent:delete` | ✓ | ✗ | ✗ | ✗ | ✗ |
| `intent:diff` | ✓ | ✓ | ✓ | ✗ | ✓ |
| `intent:rebase-preview` | ✓ | ✓ | ✓ | ✗ | ✓ |
| `intent:rebase-apply` | ✓ | ✓* | ✗ | ✗ | ✓** |

*Developer can apply low/medium risk rebases  
**Security reviewer can apply any risk rebase

### Artifact Operations

| Action | Admin | Developer | Operator | Viewer | Security Reviewer |
|--------|-------|-----------|----------|--------|-------------------|
| `artifact:create` | ✓ | ✓ | ✗ | ✗ | ✗ |
| `artifact:read` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `artifact:delete` | ✓ | ✗ | ✗ | ✗ | ✗ |
| `artifact:quarantine` | ✓ | ✗ | ✓ | ✗ | ✓ |
| `artifact:release` | ✓ | ✗ | ✓ | ✗ | ✓ |
| `artifact:provenance` | ✓ | ✓ | ✓ | ✓ | ✓ |

### Approval Operations

| Action | Admin | Developer | Operator | Viewer | Security Reviewer |
|--------|-------|-----------|----------|--------|-------------------|
| `approval:request` | ✓ | ✓ | ✗ | ✗ | ✓ |
| `approval:grant` | ✓ | ✗ | ✗ | ✗ | ✓ |
| `approval:revoke` | ✓ | ✗ | ✗ | ✗ | ✓ |
| `approval:read` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `approval:revalidate` | ✓ | ✗ | ✗ | ✗ | ✓ |

### Audit Operations

| Action | Admin | Auditor | Security Reviewer |
|--------|-------|---------|-------------------|
| `audit:read` | ✓ | ✓ | ✓ |
| `audit:export` | ✓ | ✓ | ✗ |
| `audit:verify-chain` | ✓ | ✗ | ✓ |

### Tenant Operations

| Action | Owner | Admin | Developer | Viewer |
|--------|-------|-------|-----------|--------|
| `tenant:manage-users` | ✓ | ✓ | ✗ | ✗ |
| `tenant:manage-roles` | ✓ | ✗ | ✗ | ✗ |
| `tenant:read` | ✓ | ✓ | ✓ | ✓ |
| `tenant:update` | ✓ | ✓ | ✗ | ✗ |
| `tenant:delete` | ✓ | ✗ | ✗ | ✗ |

---

## Risk-Based Authorization

### Rebase Risk Levels

| Risk Level | Apply Requires | Notification |
|------------|---------------|--------------|
| Low | Developer+ | Log only |
| Medium | Developer+ | Log + Webhook |
| High | Security Reviewer+ | Log + Webhook + Approval |
| Critical | Security Reviewer + Owner | Log + Webhook + Approval + Explicit Owner |

### Override Conditions

- **Emergency override**: Security incidents may allow elevated permissions for limited time
- **Break-glass procedure**: Requires dual approval and is logged with reason
- **Automation accounts**: Service accounts have restricted scopes matching their function

---

## API Key Scopes

| Scope | Permissions |
|-------|-------------|
| `intent:read` | Read intents, diffs, rebase previews |
| `intent:write` | Create, update intents |
| `intent:rebase` | Apply rebases (risk-limited) |
| `artifact:read` | Read artifacts, provenance |
| `audit:read` | Read audit events |
| `admin` | All operations (restricted to admin API keys) |

---

## Implementation

### JWT Claims Structure

```json
{
  "sub": "user-uuid",
  "tenant_id": "tenant-uuid",
  "roles": ["tenant:developer", "tenant:security-reviewer"],
  "scopes": ["intent:write", "artifact:read"],
  "exp": 1700000000,
  "iat": 1699990000
}
```

### Authorization Middleware

```rust
async fn authorize(
    claims: &JwtClaims,
    resource: &Resource,
    action: &Action,
) -> Result<bool, AuthorizationError> {
    // 1. Check tenant scope
    if !claims.tenant_scope_covers(resource.tenant_id) {
        return Err(AuthorizationError::TenantMismatch);
    }
    
    // 2. Check role permissions
    if !has_role_permission(&claims.roles, resource, action) {
        return Err(AuthorizationError::InsufficientRole);
    }
    
    // 3. Check risk level for rebase-apply
    if action == Action::RebaseApply {
        return authorize_risk_based(claims, resource)?;
    }
    
    Ok(true)
}
```

---

## Related Documents

- [04 — Approval Scope & Revalidation](./04-approval-revalidation.md)
- [08 — Tenant Isolation](./08-tenant-isolation.md)
- [06 — Threat Model v2](./06-threat-model-v2.md)