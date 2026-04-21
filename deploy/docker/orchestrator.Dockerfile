# idiolect-orchestrator — read-only HTTP catalog over the firehose.
#
# Two-stage build. The builder stage compiles the daemon binary with
# the `daemon` feature set (axum HTTP, sqlite catalog, tapped
# firehose, tokio multi-thread runtime). The runtime stage is
# distroless/cc:debian12 so the image ships only the binary plus the
# CA trust store the runtime uses for outbound PDS connections.
#
# Build:
#   docker build \
#     -f deploy/docker/orchestrator.Dockerfile \
#     -t idiolect-orchestrator:local .
#
# Run:
#   docker run --rm -p 8787:8787 \
#     -v idiolect-orch-data:/var/lib/idiolect/orchestrator \
#     -e IDIOLECT_CATALOG_SQLITE=/var/lib/idiolect/orchestrator/catalog.db \
#     -e IDIOLECT_CURSORS=/var/lib/idiolect/orchestrator/cursors.db \
#     -e IDIOLECT_HTTP_ADDR=0.0.0.0:8787 \
#     idiolect-orchestrator:local

FROM rust:1.85-slim-bookworm AS builder
WORKDIR /src

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Cache-friendly build: copy manifests first, cache-build a stub,
# then copy sources. `cargo build -p` only compiles the requested
# crate + its deps so the full workspace does not rebuild.
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY lexicons lexicons
COPY orchestrator-spec orchestrator-spec
COPY observer-spec observer-spec
COPY verify-spec verify-spec
COPY .cargo .cargo
COPY rust-toolchain.toml ./

RUN cargo build \
    --release \
    -p idiolect-orchestrator \
    --features daemon,catalog-sqlite,query-http \
    --bin idiolect-orchestrator

# -----------------------------------------------------------------

FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app

COPY --from=builder /src/target/release/idiolect-orchestrator /usr/local/bin/idiolect-orchestrator

USER nonroot:nonroot
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/idiolect-orchestrator"]
