---
name: Bug report
about: Report a defect or unexpected behavior in Intent Rebase Engine
title: "[bug] "
labels: []
assignees: []
---

> **Before opening a bug report:** please read the
> [Capability Support Matrix](https://github.com/BrianNguyen29/intent-rebase/blob/main/docs/01-product/04-capability-support-matrix.md).
> IRE is a bounded personal project and is **not production-ready**.
> Bugs that only manifest when treating IRE as production infrastructure
> (production-scale load, external security review, full replay,
> staging/production environment behavior, etc.) are **out of scope** for
> this project and will be closed as such. Please report only bugs that
> reproduce in local development or in the bounded local stack.

## Environment

- IRE version / commit: <!-- e.g. commit hash, `git rev-parse HEAD` -->
- Rust toolchain: <!-- output of `rustc --version` and `cargo --version` -->
- OS / architecture: <!-- e.g. Ubuntu 22.04 x86_64, macOS 14 arm64 -->
- Local stack: <!-- e.g. docker compose up Postgres + NATS + MinIO; or in-memory only -->
- Feature flags / env gates: <!-- e.g. INTENT_API_NATS_CONSUMER=true, FORENSIC_BUNDLE_STORAGE=s3 -->

## What happened

<!-- Clear, minimal description of the bug. -->

## What you expected

<!-- What you expected to happen. -->

## Reproduction

<!-- Minimal reproduction. Include the exact command(s) you ran, the
     relevant env vars, the request payload (redact secrets), and the
     response. -->

```bash
# 1. command(s) you ran
# 2. env vars set
# 3. observed output / response
```

If the bug is in source code (panic, wrong result, etc.), include:

- Crate and approximate file / function.
- A minimal test case, if you can write one.

## Local verification before opening

Please confirm you ran the fast verification locally on the commit where you
saw the bug. Paste the result of each:

- [ ] `bash scripts/verify-fast.sh` — result:
- [ ] `git diff --check` — result:
- [ ] Other relevant commands:

## Scope of the bug

- [ ] Reproduces in **in-memory / unit tests only** (`cargo test --workspace --lib --all-features`)
- [ ] Reproduces in **bounded local stack** (docker compose + `#[ignore]`'d tests)
- [ ] Reproduces **only** when treating IRE as production infrastructure (out of scope — see top of template)

## Impact and workarounds

<!-- Describe the practical impact (data loss? incorrect result? panic?)
     and any workarounds you have found. -->

## Additional context

<!-- Logs, stack traces, screenshots, related issues / PRs. Please redact
     any secrets before pasting. -->
