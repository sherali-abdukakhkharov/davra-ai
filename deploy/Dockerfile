# Builds the davra-hub image (API + web app). Context: repository root.

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

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --no-create-home davra
COPY --from=build /davra-hub /usr/local/bin/davra-hub
COPY --from=web /web/dist /usr/share/davra/web
ENV DAVRA_WEB_DIR=/usr/share/davra/web
USER davra
EXPOSE 8080
ENTRYPOINT ["davra-hub"]
CMD ["serve"]
