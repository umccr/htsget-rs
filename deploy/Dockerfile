FROM rust:1.73.0 AS builder

WORKDIR /build

RUN cargo install cargo-strip

COPY . .

RUN cargo build --features url-storage --release && \
    cargo strip

FROM debian:sid-slim

RUN apt update && apt install -y libc6-dev && rm -rf /var/lib/apt/lists/*

COPY --from=builder /etc/ssl/certs/ /etc/ssl/certs/
COPY --from=builder /build/target/release/htsget-actix /usr/local/bin/htsget-actix

EXPOSE 8080
EXPOSE 8081

CMD [ "htsget-actix" ]
