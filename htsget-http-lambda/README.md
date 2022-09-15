# htsget-http-lambda

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/action.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain

Framework dependent code for a cloud-based instance of [htsget-rs], using AWS Lambda.

[htsget-rs]: ../

## Overview

This crate is used for running a cloud-based instance of htsget-rs. It:
* Uses the [Rust Runtime for AWS Lambda][aws-lambda-rust-runtime] to produce a Lambda function which can be deployed to AWS.
* It is written as a single Lambda function which uses [htsget-http-core] to respond to queries.

[aws-lambda-rust-runtime]: https://github.com/awslabs/aws-lambda-rust-runtime
[htsget-http-core]: ../htsget-http-core

## Usage

### For running htsget-rs as an application

This crate can be deployed to AWS as a Lambda function, or interacted with locally using [cargo-lambda]. See [deploy] 
for more details.

See [htsget-search] for details on how to structure files.

[cargo-lambda]: https://github.com/cargo-lambda/cargo-lambda
[deploy]: ../deploy
[htsget-search]: ../htsget-search

### As a library

There shouldn't be any need to interact with this crate as a library, however some functions which deal with
routing queries are exposed in the public API.

#### Feature flags

This crate has the following features:
* `s3-storage`: used to enable `AwsS3Storage` functionality.

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE