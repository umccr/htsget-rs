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

This crate can be deployed to AWS as a Lambda function, or interacted with locally using [cargo-lambda]. See the [htsget-deploy] 
for more details. Note, this crate does not use any configuration relating to the local data server. CORS configuration
uses values from the ticket server config. See [htsget-config] for more information about configuration.

See [htsget-search] for details on how to structure files.

[cargo-lambda]: https://github.com/cargo-lambda/cargo-lambda
[htsget-deploy]: https://github.com/umccr/htsget-deploy
[htsget-search]: ../htsget-search
[htsget-config]: ../htsget-config

### As a library

There is no need to interact with this crate as a library. Note that the Lambda function itself doesn't have any
library code, and it instead uses `htsget-axum`. Please use that crate for functionality related to routing.

#### Feature flags

This crate has the following features:
* `s3`: used to enable `S3` location functionality and any other AWS features.
* `url`: used to enable `Url` location functionality.
* `experimental`: used to enable experimental features that aren't necessarily part of the htsget spec, such as Crypt4GH support through `C4GHStorage`.

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE