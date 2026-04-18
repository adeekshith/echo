FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev build-base

WORKDIR /app

# Cache dependency compilation
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && \
    echo 'fn main() {}' > src/main.rs && \
    echo '' > src/lib.rs && \
    cargo build --release 2>/dev/null; \
    rm -rf src && \
    rm -rf target/release/.fingerprint/ipecho* \
           target/release/deps/ipecho* \
           target/release/ipecho*

# Copy real source and build
COPY src ./src
RUN cargo build --release

FROM alpine:3.21

RUN apk add --no-cache ca-certificates wget && \
    adduser -D -u 10001 ipecho

WORKDIR /app

COPY --from=builder /app/target/release/ipecho /usr/local/bin/ipecho

ENV PORT=8083 \
    LOG_LEVEL=info \
    SYNC_INTERVAL_SECS=43200

USER ipecho

EXPOSE 8083

HEALTHCHECK --interval=30s --timeout=3s --start-period=30s --retries=3 \
    CMD wget -q -O- http://localhost:8083/health >/dev/null || exit 1

ENTRYPOINT ["ipecho"]
