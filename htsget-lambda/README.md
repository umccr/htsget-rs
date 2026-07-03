# htsget-lambda

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/action.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain

Framework dependent code for a cloud-based instance of [htsget-rs], using AWS Lambda.

[htsget-rs]: https://github.com/umccr/htsget-rs

## Overview

This crate is used for running a cloud-based instance of htsget-rs. It:
* Uses the [Rust Runtime for AWS Lambda][aws-lambda-rust-runtime] to produce a Lambda function which can be deployed to AWS.
* It is written as a single Lambda function which uses [htsget-http] to respond to queries.

[aws-lambda-rust-runtime]: https://github.com/awslabs/aws-lambda-rust-runtime
[htsget-http]: ../htsget-http

## Usage

This crate is intended to be deployed to AWS as a Lambda function. It is configured in the same
way as [htsget-axum], by using the [htsget-config]. However, given that it is a Lambda function, environment
based config is recommended over TOML files.

Pre-built Lambda deployment packages are attached to each `htsget-lambda` [release][releases] as
`htsget-lambda-v<version>-<arch>.zip` files for `arm64` and `x86_64`.

See [htsget-search] for details on how to structure files.

#### Development

This crate can be locally compiled and tested using [cargo-lambda]. For example, run the function
locally:

```sh
cargo lambda watch -p htsget-lambda
```

Then query it through the local URL:

```sh
curl localhost:9000/lambda-url/htsget-lambda/reads/service-info
```

Environment variables can be set when using [cargo-lambda]. `cargo lambda invoke`
can be used to send raw Lambda events such as API Gateway requests to the function.

[cargo-lambda]: https://github.com/cargo-lambda/cargo-lambda
[htsget-deploy]: https://github.com/umccr/htsget-deploy
[htsget-search]: ../htsget-search
[htsget-config]: ../htsget-config
[htsget-axum]: ../htsget-axum
[releases]: https://github.com/umccr/htsget-rs/releases

### As a library

There is no need to interact with this crate as a library. Note that the Lambda function itself doesn't have any
library code, and it instead uses `htsget-axum`. Please use that crate for functionality related to routing.

#### Feature flags

This crate has the following features:
* `aws`: used to enable `S3` location functionality and any other AWS features.
* `url`: used to enable `Url` location functionality.
* `experimental`: used to enable experimental features that aren't necessarily part of the htsget spec, such as Crypt4GH support through `C4GHStorage`.

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE