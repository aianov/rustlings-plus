# Сборка: отдельный образ с компилятором, в рабочий он не попадает.
FROM rust:1.88 AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY ms/post-service/Cargo.toml ms/post-service/Cargo.toml
COPY tools/healthcheck/Cargo.toml tools/healthcheck/Cargo.toml
RUN mkdir -p ms/post-service/src tools/healthcheck/src \
    && echo "fn main() {}" > ms/post-service/src/main.rs \
    && echo "fn main() {}" > tools/healthcheck/src/main.rs \
    && cargo build --release -p post-service -p healthcheck
COPY ms ms
COPY tools tools
RUN touch ms/post-service/src/main.rs tools/healthcheck/src/main.rs \
    && cargo build --release -p post-service -p healthcheck

# Рабочий образ: только бинарники.
FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --no-create-home app
COPY --from=builder /build/target/release/post-service /usr/local/bin/post-service
COPY --from=builder /build/target/release/healthcheck /usr/local/bin/healthcheck
USER app
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=3s --retries=3 CMD ["/usr/local/bin/healthcheck", "127.0.0.1:8080", "/ready"]
CMD ["post-service"]
