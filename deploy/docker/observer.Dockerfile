# idiolect-observer — firehose-to-observation daemon.
#
# Same shape as the orchestrator image: two-stage build, distroless
# runtime. Runs the observer binary with the `daemon` feature so the
# tokio runtime, tapped firehose adapter, sqlite cursor store, and
# atrium publisher are all linked in.
#
# Build:
#   docker build \
#     -f deploy/docker/observer.Dockerfile \
#     -t idiolect-observer:local .
#
# Run:
#   docker run --rm \
#     -v idiolect-obs-data:/var/lib/idiolect/observer \
#     -e IDIOLECT_OBSERVER_CURSORS=/var/lib/idiolect/observer/cursors.db \
#     -e IDIOLECT_PDS_URL=https://bsky.social \
#     -e IDIOLECT_OBSERVER_DID=did:plc:your-observer-did \
#     idiolect-observer:local

FROM rust:1.85-slim-bookworm AS builder
WORKDIR /src

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

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
    -p idiolect-observer \
    --features daemon \
    --bin idiolect-observer

# -----------------------------------------------------------------

FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app

COPY --from=builder /src/target/release/idiolect-observer /usr/local/bin/idiolect-observer

USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/idiolect-observer"]
