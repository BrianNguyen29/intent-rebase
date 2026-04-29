# Environments

## Required environments
- local
- dev shared
- ephemeral preview
- staging
- pre-prod
- production

## Environment rules
- configs as code
- secrets from vault
- no shared prod credentials
- synthetic test tenants in non-prod
- replay testing in pre-prod for risky changes

## Promotion flow
dev -> preview -> staging -> pre-prod -> prod

## Staging scaffold status

**Location:** `infrastructure/staging/docker-compose.yml`

| Field | Value |
|-------|-------|
| **Scaffold exists** | ✅ Yes (`infrastructure/staging/`) |
| **Production-ready** | ❌ No — requires external SRE/security/load/pen gates |
| **Evidence strength** | Local docker-compose (staging-like) — NOT production-equivalent |
| **Last Updated** | April 2026 |

### Staging environment gates (not yet passed)

External gates required before production consideration:

- [ ] External SRE sign-off
- [ ] External security review / pen test
- [ ] External load testing (L3+)
- [ ] Compliance checklist completion

**Note:** Do not claim `infrastructure/local/docker-compose.yml` as staging. The local stack is for local development only. Use `infrastructure/staging/docker-compose.yml` for staging-like evidence collection.
