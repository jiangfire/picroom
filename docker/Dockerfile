# Picroom Dockerfile
# Multi-stage build for a small, secure runtime image.

# ============================================================================
# Stage 1: builder
# ============================================================================
FROM rust:1.83-bookworm AS builder

# Build-time dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        ca-certificates \
        cmake \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Layer-cache dependencies first
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates crates
COPY bin bin
COPY migrations migrations

# Pre-build with stub main.rs to populate target/ with all deps
RUN mkdir -p bin/picroom/src \
    && echo 'fn main() {}' > bin/picroom/src/main.rs \
    && cargo build --release --bin picroom \
    && rm -rf bin/picroom/src/main.rs

# Now build the real thing
COPY . .
RUN cargo build --release --bin picroom \
    && strip target/release/picroom

# ============================================================================
# Stage 2: runtime
# ============================================================================
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

# Runtime metadata
LABEL org.opencontainers.image.title="Picroom"
LABEL org.opencontainers.image.description="Self-hosted image hosting service"
LABEL org.opencontainers.image.licenses="MIT"
LABEL org.opencontainers.image.source="https://github.com/picroom/picroom"

# Copy the binary
COPY --from=builder /build/target/release/picroom /usr/local/bin/picroom

# distroless already includes ca-certificates
# Non-root user (UID 65532 = nonroot) is built into the base image

# Expose default port
EXPOSE 8080

# Health check (distroless lacks curl; we use a tiny Rust healthcheck binary
# in real deployment, or rely on K8s readinessProbe).
# HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
#     CMD ["/usr/local/bin/picroom", "healthcheck"]

# Default entrypoint
ENTRYPOINT ["/usr/local/bin/picroom"]
CMD ["api", "--config", "/etc/picroom/config.toml"]