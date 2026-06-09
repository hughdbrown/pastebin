# ---- Build stage ----
# Pin a Rust that supports the 2024 edition (>= 1.96, per Cargo.toml rust-version).
FROM rust:1.96-bookworm AS builder

WORKDIR /app

# Cache dependencies: copy manifests first and build a stub so the dependency
# layer only rebuilds when Cargo.toml/Cargo.lock change.
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src \
    && echo 'fn main() {}' > src/main.rs \
    && echo '' > src/lib.rs \
    && cargo build --release \
    && rm -rf src

# Now copy the real sources and build the actual binary.
COPY src ./src
# Touch so cargo notices the changed mtime over the stub.
RUN touch src/main.rs src/lib.rs && cargo build --release

# ---- Runtime stage ----
# SQLite is statically linked via rusqlite's `bundled` feature, so the runtime
# image only needs CA certs and libgcc. A slim Debian keeps it small but glibc-
# compatible with the build stage.
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 appuser

WORKDIR /app

# Persist the SQLite database in a mounted volume.
RUN mkdir -p /data && chown appuser:appuser /data
VOLUME ["/data"]

COPY --from=builder /app/target/release/pastebin /usr/local/bin/pastebin

USER appuser

# Defaults; override at runtime with -e.
ENV BIND_ADDR=0.0.0.0:8080 \
    DATABASE_PATH=/data/pastebin.db \
    PUBLIC_BASE_URL=http://localhost:8080

EXPOSE 8080

CMD ["pastebin"]
