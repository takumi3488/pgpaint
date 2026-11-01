FROM oven/bun:1@sha256:1d22548f177f4ff38e0c15baa3c2c46c6c2060ba2d1bc573bb4720b407548f1d AS web-builder

WORKDIR /app/web

COPY web/package.json web/bun.lock ./

RUN bun install --frozen-lockfile

COPY web/ ./

RUN bun run build

# Chef stage: cargo-chef is preinstalled
FROM lukemathwalker/cargo-chef:latest-rust-1.99-alpine@sha256:2c27e67049f17e8b67d882a2cf2f5e1a1d4d71c206ec97f91edf278254688e95 AS chef

WORKDIR /app

# Planner stage: Prepare recipe.json
FROM chef AS planner

COPY . .

RUN cargo chef prepare --recipe-path recipe.json

# Builder stage: Build dependencies and application
FROM chef AS builder

# Install required dependencies for musl build
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static

COPY --from=planner /app/recipe.json recipe.json

# Build dependencies (this layer will be cached)
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json

# Copy application sources, including app/, before the Gluon macro is expanded.
# The generated public/assets are also compiled into the binary.
COPY . .
COPY --from=web-builder /app/public/assets ./public/assets

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo build --release && \
    cp /app/target/release/pgpaint /tmp/pgpaint

# Runtime stage: Create minimal production image with static binary
FROM gcr.io/distroless/static-debian13:nonroot@sha256:25ac8f0c3b2334a9eb05452266bd4697864a8c8ef0ef71e5e6480d7f92868329

WORKDIR /app

COPY --from=builder /tmp/pgpaint /usr/local/bin/pgpaint

CMD ["/usr/local/bin/pgpaint"]

