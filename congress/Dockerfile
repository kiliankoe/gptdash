FROM rust:1.92-bookworm AS builder
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && apt-get install -y nodejs
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs ./
COPY package.json package-lock.json ./
COPY scripts ./scripts
COPY static/js ./static/js
COPY static/beamer.html static/host.html ./static/
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
ARG GIT_HASH=unknown
COPY --from=builder /build/target/release/gptdash .
COPY static ./static
COPY --from=builder /build/static/js/dist ./static/js/dist
COPY --from=builder /build/static/dist ./static/dist
RUN sed -i "s/__GIT_HASH__/$GIT_HASH/g" static/dist/host.html
EXPOSE 6573
CMD ["./gptdash"]
