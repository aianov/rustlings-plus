FROM rust:1.88
WORKDIR /app
COPY . .
COPY .env .env
ENV DATABASE_PASSWORD=hunter2
RUN cargo build
CMD ./target/debug/post-service
