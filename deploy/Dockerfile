FROM rust:1.76-slim AS builder

WORKDIR /build

RUN cargo install cargo-strip

COPY . .

RUN cargo build --all-features --release && \
    cargo strip

FROM gcr.io/distroless/cc-debian12

COPY --from=builder /build/target/release/htsget-actix /usr/local/bin/htsget-actix

ENV HTSGET_TICKET_SERVER_ADDR 0.0.0.0:8080
ENV HTSGET_DATA_SERVER_ADDR 0.0.0.0:8081

CMD [ "htsget-actix" ]
