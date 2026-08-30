# syntax=docker/dockerfile:1.7
FROM oven/bun:1.4.0-alpine AS console-build
WORKDIR /source/console
COPY console/package.json console/bun.lock ./
RUN --mount=type=cache,target=/root/.bun/install/cache bun install --frozen-lockfile
COPY console ./
RUN bun run build

FROM rust:1.98.0-alpine AS hub-build
WORKDIR /source
ARG BUILD_VERSION
ENV BUILD_VERSION=$BUILD_VERSION
RUN apk add --no-cache git musl-dev
COPY .git ./.git
COPY Cargo.toml Cargo.lock ./
COPY shared ./shared
COPY hub ./hub
COPY probe ./probe
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/source/target \
    cargo build --locked --release -p gauges-hub && \
    cp target/release/gauges-hub /tmp/gauges-hub

FROM alpine:3.24
RUN addgroup -S gauges \
    && adduser -S -G gauges -h /data gauges \
    && mkdir -p /data /opt/gauges/console \
    && chown -R gauges:gauges /data /opt/gauges
COPY --from=hub-build /tmp/gauges-hub /usr/local/bin/gauges-hub
COPY --from=console-build /source/console/build /opt/gauges/console
COPY LICENSE COPYRIGHT AI_USAGE.md /usr/share/licenses/gauges/
USER gauges
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s --start-period=3s --retries=3 \
    CMD wget -q -O /dev/null http://127.0.0.1:8080/health || exit 1
ENTRYPOINT ["/usr/local/bin/gauges-hub"]
