# Pull Request

> **Reminder:** Intent Rebase Engine is **not production-ready**. It has
> had no external SRE / security sign-off, no production-scale load
> testing, and no penetration testing. See
> [Status & Capabilities](docs/reference/status-and-capabilities.md) and the
> [Capability Support Matrix](docs/01-product/04-capability-support-matrix.md).
> **Do not** introduce language implying CI is green, the project is
> production-ready, or that external review has been performed.

## Summary

<!-- One short paragraph describing what this PR changes and why. -->

## Related issues / ADRs

<!-- Link any issues, ADRs in `docs/13-adrs/`, or capability-matrix rows
     that this PR touches. -->

## Type of change

- [ ] Bug fix (bounded, non-production)
- [ ] New feature (bounded slice)
- [ ] Documentation / community-health only
- [ ] Refactor (no behavior change)
- [ ] Test-only change
- [ ] Other (describe below)

## Local verification (required)

The GitHub Actions smoke run is **not** a green-build guarantee. Local
verification is the source of truth. Please confirm you ran each of these on
the branch you are merging:

- [ ] `bash scripts/verify-fast.sh` — passes locally
- [ ] `git diff --check` — no conflict markers
- [ ] OpenAPI spectral lint (only if `docs/04-api/openapi.yaml` changed):
  `npx @stoplight/spectral-cli lint docs/04-api/openapi.yaml --ruleset .spectral.yml --fail-severity=error`
- [ ] Any `#[ignore]`'d test directly relevant to the change (e.g. RLS,
  migration, NATS) — describe what you ran and the result:

  <!-- Example: cargo test --test rls_integration -- --ignored → 4/4 passed -->

## Repository-specific rules

Please tick the rules that apply and confirm they are met:

- [ ] If this PR changes the **intent schema**, I have updated or added an
  ADR under `docs/13-adrs/` first.
- [ ] If this PR changes the **API surface**, I have updated
  `docs/04-api/openapi.yaml` and the relevant event contracts.
- [ ] If this PR changes **graph rules**, I have included tests in the same
  PR.
- [ ] If this PR changes a **risky apply-path**, I have included replay
  tests.
- [ ] This PR does **not** add S3/S4 side-effect auto-compensation. (If it
  does, explicit approval is required — please call it out in the PR
  description.)

## Documentation

- [ ] I have updated the relevant docs (architecture, API, ops, ADRs,
  capability matrix, or status & capabilities) in this same PR.
- [ ] I have **not** added any badge, status indicator, or wording that
  implies the project is CI-green, production-ready, or externally signed
  off.
- [ ] If this PR partially closes an external sign-off gap (SRE, security,
  load, or penetration testing), I have explicitly called that out in the
  PR description — the maintainer will decide whether to accept the claim.

## Checklist

- [ ] The PR is small and focused (one logical change).
- [ ] Commit messages reference an issue or ADR where applicable.
- [ ] No secrets, real credentials, or production data are included.
- [ ] I have read [CONTRIBUTING.md](CONTRIBUTING.md) and
  [AGENTS.md](AGENTS.md).
