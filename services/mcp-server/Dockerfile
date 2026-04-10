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
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 curl && rm -rf /var/lib/apt/lists/*
# Install Node.js 22 LTS for build-verify-tx.ts (Switchboard SDK subprocess)
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*
RUN npm install -g tsx
RUN useradd -r -m -s /bin/false appuser
COPY --from=test /app/target/release/mcp-server /usr/local/bin/mcp-server
# Install TS script + deps (node_modules next to the script so tsx can resolve them)
COPY services/mcp-server/scripts /home/appuser/scripts
RUN cd /home/appuser/scripts && npm install \
    && chown -R appuser:appuser /home/appuser
ENV BUILD_VERIFY_SCRIPT=/home/appuser/scripts/build-verify-tx.ts
USER appuser
WORKDIR /home/appuser
EXPOSE 8080
CMD ["mcp-server"]
