FROM rust:1.97-trixie AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/relay-server /usr/local/bin/app
WORKDIR /
EXPOSE 8080/udp
EXPOSE 8081/tcp
ENTRYPOINT ["/usr/local/bin/app"]
