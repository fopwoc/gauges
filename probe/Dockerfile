# syntax=docker/dockerfile:1.7
FROM rust:1.90-alpine AS build
WORKDIR /source
RUN apk add --no-cache musl-dev
COPY Cargo.toml Cargo.lock ./
COPY shared ./shared
COPY hub ./hub
COPY probe ./probe
RUN cargo build --locked --release -p gauges-probe

FROM alpine:3.22
RUN apk add --no-cache ca-certificates \
    && addgroup -S gauges \
    && adduser -S -G gauges -h /var/lib/gauges gauges \
    && mkdir -p /var/lib/gauges \
    && chown gauges:gauges /var/lib/gauges
COPY --from=build /source/target/release/gauges-probe /usr/local/bin/gauges-probe
USER gauges
ENTRYPOINT ["/usr/local/bin/gauges-probe"]
