# syntax=docker/dockerfile:1.4

# Use cargo-chef to cache dependency builds between Docker layers
FROM lukemathwalker/cargo-chef:latest-rust-1.86 AS chef

WORKDIR /usr/src/app
ENV CARGO_HOME=/usr/local/cargo
ENV CARGO_TARGET_DIR=/usr/src/app/target

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cacher
COPY --from=planner /usr/src/app/recipe.json recipe.json
RUN --mount=type=cache,id=tc-api-target,target=/usr/src/app/target \
    --mount=type=cache,id=tc-api-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=tc-api-git,target=/usr/local/cargo/git \
    cargo chef cook --release --recipe-path recipe.json

FROM rust:1.86 AS builder

WORKDIR /usr/src/app
ENV CARGO_HOME=/usr/local/cargo
ENV CARGO_TARGET_DIR=/usr/src/app/target
COPY . .

ENV SQLX_OFFLINE=true
RUN --mount=type=cache,id=tc-api-target,target=/usr/src/app/target \
    --mount=type=cache,id=tc-api-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=tc-api-git,target=/usr/local/cargo/git \
    cargo install sqlx-cli --locked --features postgres
RUN --mount=type=cache,id=tc-api-target,target=/usr/src/app/target \
    --mount=type=cache,id=tc-api-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=tc-api-git,target=/usr/local/cargo/git \
    cargo sqlx prepare --check
RUN --mount=type=cache,id=tc-api-target,target=/usr/src/app/target \
    --mount=type=cache,id=tc-api-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=tc-api-git,target=/usr/local/cargo/git \
    cargo build --release && \
    install -m 0755 target/release/tinycongress-api /usr/local/bin/tinycongress-api && \
    install -m 0755 target/release/client /usr/local/bin/client

FROM debian:bullseye-slim

RUN apt-get update \
  && apt-get install -y ca-certificates libpq5 libssl1.1 \
  && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/tinycongress-api /usr/local/bin/
COPY --from=builder /usr/local/bin/client /usr/local/bin/
COPY --from=builder /usr/src/app/migrations /usr/local/migrations

ENV SQLX_OFFLINE=true \
    MIGRATIONS_DIR=/usr/local/migrations

CMD ["tinycongress-api"]
