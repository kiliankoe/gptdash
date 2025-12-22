FROM rust:1.92-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
ARG GIT_HASH=unknown
COPY --from=builder /build/target/release/gptdash .
COPY static ./static
RUN sed -i "s/__GIT_HASH__/$GIT_HASH/g" static/host.html
EXPOSE 6573
CMD ["./gptdash"]
