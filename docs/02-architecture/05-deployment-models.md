# Deployment Models

## Model A — Single-tenant self-hosted
Best for:
- enterprise regulated
- air-gapped or semi-isolated
- many custom integrations

Advantages:
- maximum data control
- easy enterprise sales

Disadvantages:
- complex operations
- difficult upgrade management

## Model B — Multi-tenant SaaS
Best for:
- startup / mid-size teams
- need fast onboarding

Advantages:
- fast deployment
- centralized telemetry
- good data network effects

Disadvantages:
- high tenant isolation requirements
- more complex compliance

## Model C — Hybrid control plane
Best for:
- metadata control plane hosted
- artifact payload / secrets self-hosted

Advantages:
- balance speed and compliance
- reduce cost of full self-hosting

## Recommendation
Start with:
- single-tenant dev/staging
- production architecture supporting gradual upgrade to hybrid or multi-tenant

## Environment Model
- local dev
- ephemeral review env
- shared staging
- pre-prod
- prod
