FROM rust:1-bookworm AS builder
# cmake + libclang required to build BoringSSL via the listings-scraper
# crate's rquest/boring-sys2 deps. The scraper crate is split out from
# mcp-server (separate workspace member, separate binary) so BoringSSL
# stays out of mcp-server's link graph — solana-sdk transitively requires
# OpenSSL 3.0 symbols (ERR_get_error_all, SSL_get1_peer_certificate, ...)
# that BoringSSL doesn't ship, and Cargo's -L ordering shadows -lssl. The
# subprocess split makes both stacks coexist. cmake compiles BoringSSL,
# libclang-dev is needed by bindgen for the BoringSSL C-header FFI (macOS
# Xcode ships it locally, Debian doesn't). Adds ~2 min to cold build.
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev cmake libclang-dev && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt clippy
WORKDIR /app
COPY . .
RUN cargo build --release -p mcp-server -p listings-scraper

FROM builder AS test
RUN cargo fmt --check
RUN cargo clippy --release --no-deps -p mcp-server -p listings-scraper -- -D warnings
RUN cargo test --release -p mcp-server -p listings-scraper -p shillbot-chain

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 curl && rm -rf /var/lib/apt/lists/*
# Install Node.js 22 LTS for the tx-client Switchboard SDK subprocess
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*
RUN npm install -g tsx pnpm@10.32.1
RUN useradd -r -m -s /bin/false appuser
COPY --from=test /app/target/release/mcp-server /usr/local/bin/mcp-server
# listings-scraper is invoked as a subprocess by mcp-server's
# fetch_moltlaunch (Cloudflare-blocked upstream needing JA3 impersonation).
COPY --from=test /app/target/release/listings-scraper /usr/local/bin/listings-scraper
# Install the generated contract bindings, shared tx client, and thin TS wrapper
# as one workspace so the server uses the same package graph as tests/publishing.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml /home/appuser/
COPY sdk/contracts /home/appuser/sdk/contracts
COPY sdk/tx-client /home/appuser/sdk/tx-client
COPY services/mcp-server/scripts /home/appuser/services/mcp-server/scripts
RUN cd /home/appuser && pnpm install --frozen-lockfile \
    && pnpm --filter @swarm-tips/contracts build \
    && pnpm --filter @swarm-tips/tx-client build \
    && chown -R appuser:appuser /home/appuser
ENV BUILD_VERIFY_SCRIPT=/home/appuser/services/mcp-server/scripts/build-verify-tx-client.ts
USER appuser
WORKDIR /home/appuser
EXPOSE 8080
CMD ["mcp-server"]
