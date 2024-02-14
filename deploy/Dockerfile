FROM rust:1.74.0 AS builder

WORKDIR /build

RUN cargo install cargo-strip

COPY . .

RUN cargo build --features url-storage,crypt4gh --release && \
    cargo strip

FROM debian:stable-slim

RUN apt update && apt install -y libc6-dev && rm -rf /var/lib/apt/lists/*

COPY --from=builder /etc/ssl/certs/ /etc/ssl/certs/
COPY --from=builder /build/target/release/htsget-actix /usr/local/bin/htsget-actix

ENV HTSGET_TICKET_SERVER_ADDR 0.0.0.0:8080
ENV HTSGET_DATA_SERVER_ADDR 0.0.0.0:8081

EXPOSE 8080
EXPOSE 8081

CMD [ "htsget-actix" ]
