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
