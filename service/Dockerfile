# Use cargo-chef to cache dependency builds between Docker layers
FROM lukemathwalker/cargo-chef:latest-rust-1.91 AS chef

WORKDIR /usr/src/app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cacher
COPY --from=planner /usr/src/app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM rust:1.91 AS builder

WORKDIR /usr/src/app
COPY --from=cacher /usr/src/app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY . .

ENV SQLX_OFFLINE=true
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
  && apt-get install -y ca-certificates libpq5 libssl3 \
  && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/tinycongress-api /usr/local/bin/
COPY --from=builder /usr/src/app/target/release/client /usr/local/bin/
COPY --from=builder /usr/src/app/migrations /usr/local/migrations

ENV SQLX_OFFLINE=true \
    MIGRATIONS_DIR=/usr/local/migrations

CMD ["tinycongress-api"]
