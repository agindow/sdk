# syntax=docker/dockerfile:1
FROM rust:1.88-alpine AS build
RUN apk add --no-cache build-base
WORKDIR /source
COPY Cargo.toml Cargo.lock ./
COPY contracts/ contracts/
COPY cells/cell-relay/ cells/cell-relay/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/source/target \
    CARGO_BUILD_JOBS=2 cargo build --locked --release -p agw-http-host --bin agindow-sidecar \
 && mkdir -p /out /socket && cp target/release/agindow-sidecar /out/agindow-sidecar \
 && chown 65532:65532 /socket

FROM scratch
COPY --from=build /out/agindow-sidecar /agindow-sidecar
COPY --from=build --chown=65532:65532 /socket /socket
USER 65532:65532
ENV AGINDOW_BIND=127.0.0.1:8789 AGINDOW_SOCKET=/socket/agindow.sock
ENTRYPOINT ["/agindow-sidecar"]
