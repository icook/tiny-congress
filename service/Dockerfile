# syntax=docker/dockerfile:1.4
# Use cargo-chef to cache dependency builds between Docker layers
# RUST_VERSION must be passed via --build-arg (sourced from mise.toml via justfile)
ARG RUST_VERSION=1.91.0
FROM lukemathwalker/cargo-chef:latest-rust-${RUST_VERSION} AS chef

WORKDIR /usr/src/app

FROM chef AS planner
# Copy workspace root and all crates
COPY Cargo.toml Cargo.lock ./
COPY service ./service
COPY crates ./crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cacher
COPY --from=planner /usr/src/app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=cargo-git \
    cargo chef cook --release --recipe-path recipe.json

FROM rust:${RUST_VERSION} AS builder

WORKDIR /usr/src/app
COPY --from=cacher /usr/src/app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
# Copy workspace root and all crates
COPY Cargo.toml Cargo.lock ./
COPY service ./service
COPY crates ./crates

ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=cargo-git \
    cargo build --release -p tinycongress-api --bin tinycongress-api --bin sim --bin demo_verifier

FROM debian:bookworm-slim

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get -o Acquire::ForceIPv4=true update \
  && apt-get -o Acquire::ForceIPv4=true install -y --no-install-recommends ca-certificates libpq5 libssl3 \
  && apt-get upgrade -y libpq5 \
  && rm -rf /var/lib/apt/lists/* \
  && groupadd --system --gid 1000 app \
  && useradd --system --uid 1000 --gid app --no-create-home app

COPY --from=builder /usr/src/app/target/release/tinycongress-api /usr/local/bin/
COPY --from=builder /usr/src/app/target/release/sim /usr/local/bin/
COPY --from=builder /usr/src/app/target/release/demo_verifier /usr/local/bin/
COPY --from=builder /usr/src/app/service/migrations /usr/local/migrations

# Build metadata (injected by CI via --build-arg)
ARG GIT_SHA=unknown
ARG BUILD_TIME=unknown
ARG APP_VERSION=dev

ENV SQLX_OFFLINE=true \
    TC_DATABASE__MIGRATIONS_DIR=/usr/local/migrations \
    GIT_SHA=${GIT_SHA} \
    BUILD_TIME=${BUILD_TIME} \
    APP_VERSION=${APP_VERSION}

USER 1000
CMD ["tinycongress-api"]
