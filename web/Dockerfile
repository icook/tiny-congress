# Version args - sourced from mise.toml via CI
ARG RUST_VERSION=1.91.0
ARG NODE_VERSION=22

# WASM build stage
FROM rust:${RUST_VERSION}-slim AS wasm-builder
WORKDIR /build

# Install wasm-pack
RUN cargo install wasm-pack

# Copy workspace root and tc-crypto crate
COPY Cargo.toml ./
COPY crates/tc-crypto ./crates/tc-crypto

# Modify workspace to only include tc-crypto (service isn't needed for WASM build)
RUN sed -i 's/members = \[.*\]/members = ["crates\/tc-crypto"]/' Cargo.toml

# Build WASM
RUN cd crates/tc-crypto && wasm-pack build --target web --release --out-dir /wasm-out

# Node build stage
FROM node:${NODE_VERSION}-slim AS builder
WORKDIR /app

# Install dependencies
COPY web/.yarn ./.yarn
COPY web/package.json web/yarn.lock web/.yarnrc.yml ./
RUN yarn install --immutable

# Copy WASM output from wasm-builder
COPY --from=wasm-builder /wasm-out ./src/wasm/tc-crypto

# Build metadata (injected by CI via --build-arg)
ARG GIT_SHA=unknown
ARG BUILD_TIME=unknown
ENV GIT_SHA=${GIT_SHA}
ENV BUILD_TIME=${BUILD_TIME}

# Copy source and build
COPY web/ ./
RUN yarn build

# Production stage
FROM nginx:alpine
RUN apk upgrade --no-cache
WORKDIR /usr/share/nginx/html

# SPA routing + runtime config cache headers
COPY web/nginx.conf /etc/nginx/conf.d/default.conf

# Copy static assets from builder
COPY --from=builder /app/dist/ ./

# Entrypoint generates /config.js from env vars, then starts nginx
COPY web/docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh

EXPOSE 80
ENTRYPOINT ["/docker-entrypoint.sh"]
