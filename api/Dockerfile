# syntax=docker/dockerfile:1.4
# Multi-stage Dockerfile for Pactum Backend
# Follows security best practices: non-root user, minimal attack surface, secret management

# =============================================================================
# Stage 1: Dependencies Cache
# =============================================================================
FROM rust:1.82-slim AS deps

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for optimal layer caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source to cache dependencies
RUN mkdir src && echo 'fn main() {}' > src/main.rs

# Build dependencies (cached layer)
RUN cargo build --release && rm -rf src

# =============================================================================
# Stage 2: Build
# =============================================================================
FROM rust:1.82-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy cached dependencies from deps stage
COPY --from=deps /app/target ./target
COPY --from=deps /usr/local/cargo/registry /usr/local/cargo/registry
COPY --from=deps /usr/local/cargo/git /usr/local/cargo/git

# Copy manifests and source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY .sqlx ./.sqlx

# Touch main.rs to ensure rebuild
RUN touch src/main.rs

# Build release binary
RUN cargo build --release --bin pactum-backend

# Strip binary to reduce size
RUN strip target/release/pactum-backend

# =============================================================================
# Stage 3: Runtime
# =============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies only
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user for security
# Using fixed UID/GID for consistency across deployments
RUN groupadd --gid 10001 --system pactum \
    && useradd --uid 10001 --system --gid pactum --home /app --shell /sbin/nologin pactum

# Create app directory with proper permissions
WORKDIR /app
RUN chown pactum:pactum /app

# Copy binary from builder stage
COPY --from=builder --chown=pactum:pactum /app/target/release/pactum-backend /usr/local/bin/pactum-backend

# Switch to non-root user
USER 10001

# Expose application port
EXPOSE 8080

# Health check endpoint
# The backend should implement a /health endpoint that returns 200 OK
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD ["/usr/local/bin/pactum-backend", "--health-check"] || exit 1

# Run the application
ENTRYPOINT ["/usr/local/bin/pactum-backend"]
