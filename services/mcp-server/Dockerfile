FROM rust:1-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt clippy
WORKDIR /app
COPY . .
RUN cargo build --release -p mcp-server

FROM builder AS test
RUN cargo fmt --check
RUN cargo clippy --release --no-deps -p mcp-server -- -D warnings
RUN cargo test --release -p mcp-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
RUN useradd -r -s /bin/false appuser
COPY --from=test /app/target/release/mcp-server /usr/local/bin/mcp-server
USER appuser
EXPOSE 8080
CMD ["mcp-server"]
