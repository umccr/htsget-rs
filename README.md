# htsget-rs

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/tests.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain


A **server** implementation of the [htsget protocol][htsget-protocol] for bioinformatics in Rust. It is:
* **Fully-featured**: supports BAM and CRAM for reads, and VCF and BCF for variants, as well as other aspects of the protocol such as TLS, and CORS.
* **Serverless**: supports local server instances using [Axum][axum] and [Actix Web][actix-web], and serverless instances using [AWS Lambda Rust Runtime][aws-lambda-rust-runtime].
* **Storage interchangeable**: supports local filesystem storage as well as objects via [Minio][minio] and [AWS S3][aws-s3].
* **Thoroughly tested and benchmarked**: tested using a purpose-built [test suite][htsget-test] and benchmarked using [criterion-rs].

[actix-web]: https://github.com/actix/actix-web
[criterion-rs]: https://github.com/bheisler/criterion.rs

## Quickstart

To run a local instance htsget-rs, run [htsget-axum]:

```sh
cargo run -p htsget-axum
```

And fetch tickets from `127.0.0.1:8080`, which serves data from [data]:

```sh
curl 'http://127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer'
```

### Configuration

Htsget-rs is configured using environment variables or config files, see [htsget-config] for details.

### Cloud

Cloud-based htsget-rs uses [htsget-lambda]. For an example deployment of this crate see [deploy].

## Protocol

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

### Tests

Tests can be run tests by executing:

```sh
cargo test --all-features
```

To run benchmarks, see the benchmark sections of [htsget-actix][htsget-actix-benches] and [htsget-search][htsget-search-benches].

[htsget-actix-benches]: htsget-actix/README.md#benchmarks
[htsget-search-benches]: htsget-search/README.md#benchmarks

## Project Layout

This repository is a workspace of crates:

- [htsget-config]: Configuration of the server.
- [htsget-actix]: Local instance of the htsget server. Contains framework dependent code using [Actix Web][actix-web].
- [htsget-axum]: Local instance of the htsget server. Contains framework dependent code using [Axum][axum].
- [htsget-http]: Handling of htsget HTTP requests. Framework independent code.
- [htsget-lambda]: Cloud-based instance of the htsget server. Contains framework dependent
code using the [Rust Runtime for AWS Lambda][aws-lambda-rust-runtime].
- [htsget-search]: Core logic needed to search bioinformatics files based on htsget queries.
- [htsget-storage]: Storage interfaces for local and cloud-based files.
- [htsget-test]: Test suite used by other crates in the project.

Other directories contain further applications or data:
- [data]: Contains example data files used by htsget-rs and in tests.
- [deploy]: Deployments for htsget-rs.

[axum]: https://github.com/tokio-rs/axum
[htsget-config]: htsget-config
[htsget-actix]: htsget-actix
[htsget-http]: htsget-http
[htsget-lambda]: htsget-lambda
[htsget-search]: htsget-search
[htsget-storage]: htsget-storage
[htsget-test]: htsget-test

[data]: data
[deploy]: deploy

[actix-web]: https://actix.rs/
[aws-lambda-rust-runtime]: https://github.com/awslabs/aws-lambda-rust-runtime

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
[aws-s3]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html
[minio]: https://min.io/