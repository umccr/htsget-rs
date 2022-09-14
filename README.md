# htsget-rs

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/action.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain


A **server** implementation of the [htsget protocol][htsget-protocol] for bioinformatics in Rust. It is:
* **Fully-featured**: supports BAM and CRAM for reads, and VCF and BCF for variants.
* **Serverless**: supports local server instances using [Actix Web][actix-web], and serverless instances using AWS Lambda.
* **Storage Interchangeable**: supports local filesystem storage, and storage on AWS S3.

To get started, see [Usage].

**Note**: htsget-rs is still experimental, and subject to change.

[actix-web]: https://github.com/actix/actix-web
[Usage]: #usage

## Overview

Htsget-rs implements the [htsget protocol][htsget-protocol], which is a HTTP-based protocol for querying bioinformatics files. 
The htsget protocol outlines how a htsget server should behave, and it is an effective way to fetch regions of large bioinformatics files. 

A htsget server responds to queries which ask for regions of bioinformatics files. It does this by returning an array of URL
tickets, that the client must fetch and concatenate. This process is outlined in the diagram below:

![htsget-ticket][htsget-ticket]

htsget-rs implements this process as closely as possible, and aims to return byte ranges that are as small as possible. 
In htsget-rs the ticket server handled by [htsget-http-actix] or [htsget-http-lambda], and the data 
block server is handled by the [storage backend][storage-backend], either [locally][local-storage], or using [AWS S3][s3-storage].
htsget-rs is written asynchronously and uses the [Tokio] runtime.

htsget-rs implements the following components of the protocol:
* `GET` requests.
* `POST` requests.
* BAM and CRAM for the `reads` endpoint.
* VCF and BCF for the `variants` endpoint.
* `service-info` endpoint. 
* TLS on the data block server. 

[htsget-protocol]: http://samtools.github.io/hts-specs/htsget.html
[htsget-ticket]: https://samtools.github.io/hts-specs/pub/htsget-ticket.png
[storage-backend]: htsget-search/src/storage
[local-storage]: htsget-search/src/storage/local.rs
[s3-storage]: htsget-search/src/storage/aws.rs
[tokio]: https://github.com/tokio-rs/tokio

## Usage

Htsget-rs is configured using environment variables, for details on how to set them, see [htsget-config].

### Local
To run a local instance htsget-rs, run [htsget-http-actix] by executing the following:
```shell
cargo run -p htsget-http-actix
```
Using the default configuration, this will start a ticket server on `127.0.0.1:8080` and a data block server on `127.0.0.1:8081`
with data accessible from the [data] directory. See [htsget-http-actix] for more information.

### Cloud
Cloud based htsget-rs uses [htsget-http-lambda]. For more information and an example deployment of this crate see 
[deploy].

### Tests

Tests can be run tests by executing:

```shell
cargo test --all-features
```

To run benchmarks, see the benchmark sections of [htsget-http-actix][htsget-http-actix-benches] and [htsget-search][htsget-search-benches].

[htsget-http-actix-benches]: htsget-http-actix/README.md#Benchmarks
[htsget-search-benches]: htsget-search/README.md#Benchmarks

## Project Layout

This repository consists of a workspace composed of the following crates:

- [htsget-config]: Configuration of the server.
- [htsget-http-actix]: Local instance of the htsget server. Contains framework dependent code using [actix-web].
- [htsget-http-core]: Handling of htsget HTTP requests. Framework independent code.
- [htsget-http-lambda]: Cloud based instance of the htsget server. Contains framework dependent
code using the [aws-lambda-rust-runtime].
- [htsget-search]: Core logic needed to search bioinformatics files based on htsget queries.
- [htsget-test-utils]: Test utilities used by other crates in the project.

Other directories contain further applications or data:
- [data]: Contains example data files which can be used by by htsget-rs, in folders denoting the file type.
This directory also contains example events used by a cloud instance of htsget-rs in the [events][data-events] subdirectory.
- [deploy]: An example deployment of [htsget-http-lambda].

[htsget-config]: htsget-config
[htsget-http-actix]: htsget-http-actix
[htsget-http-core]: htsget-http-core
[htsget-http-lambda]: htsget-http-lambda
[htsget-search]: htsget-search
[htsget-test-utils]: htsget-test-utils

[data]: data
[deploy]: deploy

[actix-web]: https://actix.rs/
[aws-lambda-rust-runtime]: https://github.com/awslabs/aws-lambda-rust-runtime
[data-events]: data/events

## Contributing

Thanks for your interest in contributing, we would love to have you! 
See the [contributing guide][contributing] for more information.

[contributing]: CONTRIBUTING.md

## Code of conduct

We follow the [Rust Code of conduct][rust-code-of-conduct]. For moderation, please contact the maintainers of this
project directly, at mmalenic1@gmail.com (@mmalenic).

[rust-code-of-conduct]: https://www.rust-lang.org/policies/code-of-conduct

## License

This project is licensed under the [MIT license][license].

[htsget-http-actix]: htsget-http-actix
[htsget-http-lambda]: htsget-http-lambda
[license]: LICENSE
