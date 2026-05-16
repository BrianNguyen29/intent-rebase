# Justfile — Intent Rebase Engine
# Mirrors scripts/verify-fast.sh as discrete targets for teams using `just`.
# Benchmarks are explicitly deferred: no benchmark harnesses exist yet.

fmt-check:
    cargo fmt --all -- --check

check:
    cargo check --workspace --all-features

clippy:
    cargo clippy --workspace --all-features -- -D warnings

test-lib:
    cargo test --workspace --lib --all-features

# Run all fast checks sequentially (no external services required)
verify-fast: fmt-check check clippy test-lib
