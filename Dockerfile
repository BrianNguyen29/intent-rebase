# =============================================================================
# Intent API Server Dockerfile
# =============================================================================
# Phase C: Minimal production-ready Docker build for intent-api server
#
# Multi-stage build:
# - Stage 1: Build the Rust application
# - Stage 2: Runtime image with minimal dependencies
#
# Build args:
#   RUST_PROFILE: Build profile to use (default: release)
#
# Example:
#   docker build -t intent-api:latest .
#   docker run -p 8080:8080 -e DATABASE_URL=postgres://... intent-api:latest
# =============================================================================

# Stage 1: Build
FROM rust:1.80-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/intent-rebase-types/Cargo.toml crates/intent-rebase-types/
COPY crates/intent-service/Cargo.toml crates/intent-service/
COPY crates/intent-api/Cargo.toml crates/intent-api/
COPY crates/rebase-engine/Cargo.toml crates/rebase-engine/
COPY crates/graph-service/Cargo.toml crates/graph-service/
COPY crates/runtime-adapter/Cargo.toml crates/runtime-adapter/
COPY crates/rebase-orchestrator/Cargo.toml crates/rebase-orchestrator/
COPY crates/compensation-service/Cargo.toml crates/compensation-service/
COPY crates/forensic-service/Cargo.toml crates/forensic-service/
COPY crates/intent-cli/Cargo.toml crates/intent-cli/

# Create dummy source files for dependency caching
RUN mkdir -p crates/intent-rebase-types/src && \
    echo "pub struct Dummy {}" > crates/intent-rebase-types/src/lib.rs && \
    mkdir -p crates/intent-service/src && \
    echo "pub struct Dummy {}" > crates/intent-service/src/lib.rs && \
    mkdir -p crates/intent-api/src && \
    echo "pub struct Dummy {}" > crates/intent-api/src/lib.rs && \
    mkdir -p crates/rebase-engine/src && \
    echo "pub struct Dummy {}" > crates/rebase-engine/src/lib.rs && \
    mkdir -p crates/graph-service/src && \
    echo "pub struct Dummy {}" > crates/graph-service/src/lib.rs && \
    mkdir -p crates/runtime-adapter/src && \
    echo "pub struct Dummy {}" > crates/runtime-adapter/src/lib.rs && \
    mkdir -p crates/rebase-orchestrator/src && \
    echo "pub struct Dummy {}" > crates/rebase-orchestrator/src/lib.rs && \
    mkdir -p crates/compensation-service/src && \
    echo "pub struct Dummy {}" > crates/compensation-service/src/lib.rs && \
    mkdir -p crates/forensic-service/src && \
    echo "pub struct Dummy {}" > crates/forensic-service/src/lib.rs && \
    mkdir -p crates/intent-cli/src && \
    echo "fn main() {}" > crates/intent-cli/src/main.rs

# Pre-build dependencies to cache
RUN cargo build --package intent-api --bin intent-api --release 2>/dev/null || true

# Copy actual source code
COPY . .

# Build the binary
RUN cargo build --package intent-api --bin intent-api --release

# Stage 2: Runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user for security
RUN groupadd --gid 1000 intent && \
    useradd --uid 1000 --gid intent --shell /bin/false --create-home intent

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/intent-api /app/intent-api

# Copy env example for reference
COPY --from=builder /app/.env.example /app/.env.example

# Set ownership
RUN chown -R intent:intent /app

# Switch to non-root user
USER intent

# Expose default port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget -q --spider http://localhost:8080/health || exit 1

# Default environment
ENV INTENT_API_BIND_ADDR=0.0.0.0:8080
ENV RUST_LOG=info

# Run the binary
ENTRYPOINT ["/app/intent-api"]
