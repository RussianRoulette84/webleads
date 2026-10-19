# webleads: the server and the page, in one image. The Google Maps scraper runs
# beside it as its own container — see docker-compose.yml.

# ── 1. Build ─────────────────────────────────────────────────────────────────
FROM rust:1-trixie AS build
ARG TARGETARCH
ARG TRUNK_VERSION=v0.21.14

# The page is Rust compiled to WebAssembly, bundled by trunk.
RUN rustup target add wasm32-unknown-unknown \
 && case "$TARGETARCH" in \
      arm64) arch=aarch64 ;; \
      *)     arch=x86_64 ;; \
    esac \
 && curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/${TRUNK_VERSION}/trunk-${arch}-unknown-linux-gnu.tar.gz" \
    | tar -xz -C /usr/local/bin

# Same folder layout as the repository: the server finds the page, the trade list
# and the database by walking up from where it was built.
WORKDIR /app
COPY src ./src
RUN cd src/webleads/crates/ui && trunk build --release
RUN cd src/webleads && cargo build --release -p webleads-server

# ── 2. Run ───────────────────────────────────────────────────────────────────
FROM debian:trixie-slim
# The browser takes the screenshots of the broken websites.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates chromium-headless-shell \
 && apt-get clean

WORKDIR /app
COPY --from=build /app/src/webleads/target/release/webleads-server /usr/local/bin/
COPY --from=build /app/src/webleads/crates/ui/dist /app/src/webleads/crates/ui/dist

EXPOSE 8100
CMD ["webleads-server"]
