# Authentication and Authorization

## Authentication
- User auth: OIDC/OAuth2
- Service auth: mTLS hoặc workload identity
- Connectors/webhooks: signed secrets + issuer validation

## Authorization model
Kết hợp:
- RBAC cho console actions
- ABAC theo tenant, workflow risk, domain, environment
- scope-based permissions cho APIs

## Permissions examples
- `intent.read`
- `intent.write`
- `rebase.preview`
- `rebase.apply.low_risk`
- `rebase.apply.high_risk`
- `approval.revalidate`
- `artifact.quarantine`
- `compensation.execute`
- `audit.export`

## Sensitive actions requiring step-up auth
- force apply high-risk rebase
- waive compensation
- export full forensic bundle
- cross-env operations
