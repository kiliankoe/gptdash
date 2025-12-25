FROM rust:1.92-bookworm AS builder
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && apt-get install -y nodejs
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs ./
COPY package.json package-lock.json ./
COPY scripts ./scripts
COPY static/js ./static/js
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
ARG GIT_HASH=unknown
COPY --from=builder /build/target/release/gptdash .
COPY static ./static
COPY --from=builder /build/static/js/dist ./static/js/dist
RUN sed -i "s/__GIT_HASH__/$GIT_HASH/g" static/host.html
EXPOSE 6573
CMD ["./gptdash"]
