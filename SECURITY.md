# Security Policy

> **IRE is not production-ready.** It has had no external SRE sign-off, no
> external security sign-off, no production-scale load testing, and no
> penetration testing. Do **not** use IRE for sensitive, customer-facing,
> regulated, or production workloads.

## Supported versions

This repository is a personal-project bounded implementation. There is **no
formal security support window** and **no SLA**. Security fixes are best
effort and bounded by the project's current scope.

| Version | Supported           | Notes                                  |
| ------- | ------------------- | -------------------------------------- |
| `main`  | Best-effort bounded | Current development; no backport tree |

## What this policy covers

This policy covers the source code, documentation, GitHub Actions
configuration, and example configurations in this repository. It does **not**
cover:

- Any production, staging, or shared environment you may have built on top
  of IRE — those are operated by you and out of scope here.
- Third-party dependencies. They are pinned via `Cargo.lock` and audited
  opportunistically, not as part of an ongoing vulnerability-management
  program.
- Operational guarantees for observability, backup, DLQ, webhooks, or
  replay. Bounded slices exist; production-grade guarantees do not.

## Reporting a vulnerability

Please report security issues privately through GitHub’s private reporting
channel if it is enabled for this repository:

- **GitHub Security Advisories** — on the repository page, go to the
  **Security** tab → **Advisories** → **New draft security advisory**. If
  the private advisory flow is not available, open a regular issue with the
  `security` label and the maintainers will triage.

If neither GitHub channel is suitable, contact the maintainer through the
contact listed on the GitHub profile associated with this repository. **Do
not** post secrets, exploit payloads, or reproducible PoCs in public
issues.

When reporting, please include:

- A short description of the issue and its impact.
- Affected crate(s), file(s), and commit or tag if known.
- Reproduction steps or a minimal proof of concept (private only).
- Environment details (local dev, your own infra, OS, Rust toolchain).
- Whether you believe the issue crosses into a non-IRE system you operate.

## What to expect

This is a personal project with limited maintainer time. Please expect:

- An **acknowledgement** within a reasonable time, but **no guaranteed
  response window**.
- Triage at the same cadence as other issue work. Security issues are
  prioritized, but fixes are still bounded by the project's current scope.
- **No coordinated public disclosure timeline** is committed in advance. The
  maintainer may publish a fix first and the advisory afterwards, or vice
  versa, depending on context.
- **No CVE assignment** is committed in advance. If a CVE is required, the
  reporter is responsible for requesting one from a CNA.

## What is explicitly **not** promised

- No external security review or sign-off.
- No penetration-test results.
- No formal vulnerability-management program, no SLA, no bounty.
- No guarantee that reported issues will be patched in any particular
  release or branch.
- No "secure by default" certification of any bounded or production-grade
  variant.

## Hardening guidance for local use

If you run IRE locally for development, please follow the project's own
guidance:

- Keep `.env` out of version control. `.gitignore` already excludes it; do
  not commit secrets.
- Treat `JWT_SECRET`, `*_ACCESS_KEY`, and `*_SECRET_KEY` in
  [`.env.example`](.env.example) as placeholders. Replace with strong values
  before any non-local use.
- Do not expose the local docker-compose stack to the public internet. It is
  not designed for that.
- For anything beyond local development, re-evaluate the entire
  [Status & Capabilities](docs/reference/status-and-capabilities.md) and the
  [Capability Support Matrix](docs/01-product/04-capability-support-matrix.md)
  before relying on the result.

Thank you for reporting responsibly.
