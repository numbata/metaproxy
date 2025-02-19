# Rust builder stage
FROM rust:1.74-slim-bullseye as builder

WORKDIR /usr/src/metaproxy

# Create a dummy project to cache dependencies
RUN cargo new .
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release

# Copy the real source code
COPY src ./src/
COPY tests ./tests/

# Build the real project
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bullseye-slim

# Install SSL certificates for HTTPS support
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /usr/src/metaproxy/target/release/metaproxy /usr/local/bin/

# Create a non-root user
RUN useradd -m -U -s /bin/false metaproxy

# Switch to non-root user
USER metaproxy

# Set environment variables
ENV PROXY_BIND_HOST=0.0.0.0
ENV PROXY_BIND_PORT=8081
ENV PROXY_REQUEST_TIMEOUT_SECS=30
ENV PROXY_POOL_IDLE_TIMEOUT_SECS=90
ENV PROXY_POOL_MAX_IDLE_PER_HOST=32

# Expose the proxy port
EXPOSE 8081

# Run the proxy
CMD ["metaproxy"]
