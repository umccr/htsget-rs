# htsget-rs

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/tests.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain


A **server** implementation of the [htsget protocol][htsget-protocol] for bioinformatics in Rust. It is:
* **Fully-featured**: supports BAM and CRAM for reads, and VCF and BCF for variants, as well as other aspects of the protocol such as TLS, and CORS.
* **Serverless**: supports local server instances using [Actix Web][actix-web], and serverless instances using [AWS Lambda Rust Runtime][aws-lambda-rust-runtime].
* **Storage interchangeable**: supports local filesystem storage as well as objects via [Minio][minio] and AWS S3.
* **Thoroughly tested and benchmarked**: tested using a purpose-built [test suite][htsget-test] and benchmarked using [criterion-rs].

To get started, see [Usage].

**Note**: htsget-rs is still experimental, and subject to change.

[actix-web]: https://github.com/actix/actix-web
[criterion-rs]: https://github.com/bheisler/criterion.rs
[Usage]: #usage

## Overview

Htsget-rs implements the [htsget protocol][htsget-protocol], which is an HTTP-based protocol for querying bioinformatics files. 
The htsget protocol outlines how a htsget server should behave, and it is an effective way to fetch regions of large bioinformatics files. 

A htsget server responds to queries which ask for regions of bioinformatics files. It does this by returning an array of URL
tickets, that the client must fetch and concatenate. This process is outlined in the [diagram below][htsget-diagram]:

![htsget-diagram][htsget-diagram-png]

htsget-rs implements this process as closely as possible, and aims to return byte ranges that are as small as possible.
htsget-rs is written asynchronously using the [Tokio] runtime. It aims to be as efficient and safe as possible, having
a thorough set of tests and benchmarks.

htsget-rs implements the following components of the protocol:
* `GET` requests.
* `POST` requests.
* BAM and CRAM for the `reads` endpoint.
* VCF and BCF for the `variants` endpoint.
* `service-info` endpoint. 
* TLS on the data block server. 
* CORS support on the ticket and data block servers.

[htsget-protocol]: http://samtools.github.io/hts-specs/htsget.html
[htsget-diagram]: http://samtools.github.io/hts-specs/htsget.html#diagram-of-core-mechanic
[htsget-diagram-png]: https://samtools.github.io/hts-specs/pub/htsget-ticket.png
[tokio]: https://github.com/tokio-rs/tokio

## Usage

Htsget-rs is configured using environment variables, for details on how to set them, see [htsget-config].

### Local
To run a local instance htsget-rs, run [htsget-actix] by executing the following:
```sh
cargo run -p htsget-actix
```
Using the default configuration, this will start a ticket server on `127.0.0.1:8080` and a data block server on `127.0.0.1:8081`
with data accessible from the [data] directory. See [htsget-actix] for more information.

### Cloud
Cloud based htsget-rs uses [htsget-lambda]. For more information and an example deployment of this crate see 
[deploy].

### Tests

Tests can be run tests by executing:

```sh
cargo test --all-features
```

To run benchmarks, see the benchmark sections of [htsget-actix][htsget-actix-benches] and [htsget-search][htsget-search-benches].

[htsget-actix-benches]: htsget-actix/README.md#Benchmarks
[htsget-search-benches]: htsget-search/README.md#Benchmarks

## Project Layout

This repository consists of a workspace composed of the following crates:

- [htsget-config]: Configuration of the server.
- [htsget-actix]: Local instance of the htsget server. Contains framework dependent code using [Actix Web][actix-web].
- [htsget-http]: Handling of htsget HTTP requests. Framework independent code.
- [htsget-lambda]: Cloud based instance of the htsget server. Contains framework dependent
code using the [Rust Runtime for AWS Lambda][aws-lambda-rust-runtime].
- [htsget-search]: Core logic needed to search bioinformatics files based on htsget queries.
- [htsget-test]: Test suite used by other crates in the project.

Other directories contain further applications or data:
- [data]: Contains example data files which can be used by htsget-rs, in folders denoting the file type.
This directory also contains example events used by a cloud instance of htsget-rs in the [`events`][data-events] subdirectory.
- [deploy]: An example deployment of [htsget-lambda].

In htsget-rs the ticket server handled by [htsget-actix] or [htsget-lambda], and the data
block server is handled by the [storage backend][storage-backend], either [locally][local-storage], or using [AWS S3][s3-storage].
This project layout is structured to allow for extensibility and modularity. For example, a new ticket server and data server could 
be implemented using Cloudflare Workers in a `htsget-http-workers` crate and Cloudflare R2 in [htsget-search].

See the [htsget-search overview][htsget-search-overview] for more information on the storage backend.

[htsget-config]: htsget-config
[htsget-actix]: htsget-actix
[htsget-http]: htsget-http
[htsget-lambda]: htsget-lambda
[htsget-search]: htsget-search
[htsget-search-overview]: htsget-search/README.md#Overview
[htsget-test]: htsget-test

[storage-backend]: htsget-search/src/storage
[local-storage]: htsget-search/src/storage/local.rs
[s3-storage]: htsget-search/src/storage/s3.rs

[data]: data
[deploy]: deploy

[actix-web]: https://actix.rs/
[aws-lambda-rust-runtime]: https://github.com/awslabs/aws-lambda-rust-runtime
[data-events]: data/events

## Contributing

Thanks for your interest in contributing, we would love to have you! 
See the [contributing guide][contributing] for more information.

[contributing]: CONTRIBUTING.md

## License

This project is licensed under the [MIT license][license].

[htsget-actix]: htsget-actix
[htsget-lambda]: htsget-lambda
[license]: LICENSE
[aws-lambda-rust-runtime]: https://github.com/awslabs/aws-lambda-rust-runtime
[minio]: https://min.io/