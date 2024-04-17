FROM rust:1.76-slim AS builder

LABEL org.opencontainers.image.source=https://github.com/umccr/htsget-rs
LABEL org.opencontainers.image.url=https://github.com/umccr/htsget-rs/pkgs/container/htsget-rs
LABEL org.opencontainers.image.description="A server implementation of the htsget protocol for bioinformatics in Rust"
LABEL org.opencontainers.image.licenses=MIT
LABEL org.opencontainers.image.authors="Roman Valls Guimera <brainstorm@nopcode.org>, Marko Malenic <mmalenic1@gmail.com>"

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
