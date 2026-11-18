# The deployment image: one binary with the web app inside (README.md «Поставка»).
# Build for the server's platform from the repository root:
#   docker buildx build --platform linux/amd64 -f deploy/Dockerfile -t prostor:1.12c --load .
# The stages follow the Makefile: `wasm` -> `web` -> `build` (docs/conventions.md).

# 1. The query language's WebAssembly module (Makefile `wasm`).
#    rustup reads rust-toolchain.toml: channel 1.99.1 plus the wasm32 target (D-014).
FROM rust:1.99.1 AS wasm
WORKDIR /work
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --locked -p prostor-query-wasm --target wasm32-unknown-unknown --profile wasm

# 2. The web app into web/build (Makefile `web`). Playwright's browsers are not needed here.
FROM node:24.22.1-bookworm-slim AS web
WORKDIR /web
ENV PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
COPY web/package.json web/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm \
    npm ci
COPY web/ .
COPY --from=wasm /work/target/wasm32-unknown-unknown/wasm/prostor_query_wasm.wasm \
     src/lib/query/query.wasm
RUN npm run -s build

# 3. The release binary with web/build embedded (Makefile `build`; .sqlx is committed,
#    so SQLX_OFFLINE needs no database). The artifact leaves the cache mount by hand.
FROM rust:1.99.1 AS server
WORKDIR /work
COPY . .
COPY --from=web /web/build web/build
ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/work/target \
    cargo build --release --locked -p prostor-server \
    && cp target/release/prostor /prostor

# 4. Runtime. Time zones are compiled into the binary (jiff tzdb-bundle-always);
#    ca-certificates are for outbound TLS (mail, Gravatar).
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --home /data --create-home prostor
COPY --from=server /prostor /usr/local/bin/prostor
USER prostor
# In a container the server must listen on all interfaces; files live on the /data volume.
ENV PROSTOR_LISTEN=0.0.0.0:7700 \
    PROSTOR_FILES_DIR=/data/files
EXPOSE 7700
ENTRYPOINT ["/usr/local/bin/prostor"]
