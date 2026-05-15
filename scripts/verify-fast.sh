#!/usr/bin/env bash
# verify-fast.sh — Lightweight repository verification
#
# Does NOT require Postgres, NATS, or any external services.
# Runs format check, type check, clippy, and in-memory lib tests.
# Intended for rapid pre-commit / CI smoke validation.

set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== verify-fast: format check ==="
cargo fmt --all -- --check

echo "=== verify-fast: cargo check (workspace, all features) ==="
cargo check --workspace --all-features

echo "=== verify-fast: cargo clippy (workspace, all features, deny warnings) ==="
cargo clippy --workspace --all-features -- -D warnings

echo "=== verify-fast: cargo test --lib (workspace, all features, in-memory only) ==="
cargo test --workspace --lib --all-features

echo "=== verify-fast: all checks passed ==="
