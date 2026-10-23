# syntax=docker/dockerfile:1.7
FROM rust:1.88-bookworm AS build
WORKDIR /backend
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY tools ./tools
COPY tests ./tests
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/backend/target \
    cargo build --release --locked -p auth-service -p healthcheck \
    && mkdir -p /out \
    && cp target/release/auth-service target/release/healthcheck /out/

FROM debian:bookworm-slim AS runtime
RUN useradd --uid 10001 --no-create-home --shell /usr/sbin/nologin social
COPY --from=build /out/auth-service /usr/local/bin/auth-service
COPY --from=build /out/healthcheck /usr/local/bin/healthcheck
ENV CERTS_DIR=/certs \
    AUTH_HTTP_ADDR=0.0.0.0:7881 \
    AUTH_GRPC_ADDR=0.0.0.0:50051 \
    AUTH_ADMIN_ADDR=0.0.0.0:9881
EXPOSE 7881 50051 9881
USER 10001
HEALTHCHECK --interval=3s --timeout=3s --start-period=5s --retries=20 \
    CMD ["healthcheck", "http://127.0.0.1:9881/ready"]
ENTRYPOINT ["/usr/local/bin/auth-service"]
