FROM rust:1.92-bookworm AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y git && rm -rf /var/lib/apt/lists/*
COPY .git ./.git
RUN git rev-parse --short HEAD > /tmp/git_hash
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /build/target/release/gptdash .
COPY --from=builder /tmp/git_hash /tmp/git_hash
COPY static ./static
RUN GIT_HASH=$(cat /tmp/git_hash) && \
    sed -i "s/__GIT_HASH__/$GIT_HASH/g" static/host.html && \
    rm /tmp/git_hash
EXPOSE 6573
CMD ["./gptdash"]
