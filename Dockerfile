# ---------------------------------------------------
# Build Stage: Compile the Rust Tantivy FTS Daemon
# ---------------------------------------------------
FROM rust:1.99.1-slim-trixie AS builder

WORKDIR /usr/src/tantivy_daemon

# Install dependencies and build the release binary
RUN apt-get update && apt-get install -y pkg-config libssl-dev git

# Copy and build Rust files
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

# ---------------------------------------------------
# Final Stage: Minimal image for the Tantivy Daemon
# ---------------------------------------------------
FROM debian:trixie-20261020-slim

# Install runtime dependencies: CA certificates and curl for the healthcheck
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*

# Copy the compiled Rust binary from the builder
COPY --from=builder /usr/src/tantivy_daemon/target/release/tantivy_daemon /usr/local/bin/tantivy_daemon

# Create a directory for the index
ENV TANTIVY_INDEX_PATH=/tantivy_index
RUN mkdir -p /tantivy_index

WORKDIR /app

EXPOSE 8080

CMD ["tantivy_daemon"]
