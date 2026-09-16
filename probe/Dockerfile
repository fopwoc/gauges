# syntax=docker/dockerfile:1.7
FROM rust:1.98.0-alpine AS build
WORKDIR /source
ARG BUILD_VERSION
ENV BUILD_VERSION=$BUILD_VERSION
RUN apk add --no-cache git musl-dev
COPY .git ./.git
COPY Cargo.toml Cargo.lock ./
COPY shared ./shared
COPY hub ./hub
COPY probe ./probe
RUN cargo build --locked --release -p gauges-probe

FROM alpine:3.24
RUN apk add --no-cache ca-certificates smartmontools \
    && addgroup -S gauges \
    && adduser -S -G gauges -h /var/lib/gauges gauges \
    && mkdir -p /var/lib/gauges \
    && chown gauges:gauges /var/lib/gauges
COPY --from=build /source/target/release/gauges-probe /usr/local/bin/gauges-probe
COPY LICENSE COPYRIGHT AI_USAGE.md /usr/share/licenses/gauges/
USER gauges
ENTRYPOINT ["/usr/local/bin/gauges-probe"]
