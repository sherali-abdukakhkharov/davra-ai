# Builds the davra-hub image: API + web app + downloadable davra clients.
# Context: repository root.

FROM node:24-bookworm-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p davra-hub \
 && cp target/release/davra-hub /davra-hub

# The davra client for people's machines. Linux is built on bullseye so the
# binary runs on older glibc (Ubuntu 22.04 and up); Windows is cross-compiled.
FROM rust:1-bullseye AS clients
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libdbus-1-dev mingw-w64 \
 && rm -rf /var/lib/apt/lists/* \
 && rustup target add x86_64-pc-windows-gnu
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN --mount=type=cache,id=davra-clients-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=davra-clients-target,target=/src/target \
    cargo build --release --locked -p davra-cli \
 && cargo build --release --locked -p davra-cli --target x86_64-pc-windows-gnu \
 && mkdir /dl \
 && cp target/release/davra /dl/davra-linux-x86_64 \
 && cp target/x86_64-pc-windows-gnu/release/davra.exe /dl/davra-windows-x86_64.exe \
 && cd /dl && sha256sum davra-linux-x86_64 davra-windows-x86_64.exe > SHA256SUMS

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --no-create-home davra
COPY --from=build /davra-hub /usr/local/bin/davra-hub
COPY --from=web /web/dist /usr/share/davra/web
COPY --from=clients /dl /usr/share/davra/downloads
ENV DAVRA_WEB_DIR=/usr/share/davra/web \
    DAVRA_DOWNLOADS_DIR=/usr/share/davra/downloads
USER davra
EXPOSE 8080
ENTRYPOINT ["davra-hub"]
CMD ["serve"]
