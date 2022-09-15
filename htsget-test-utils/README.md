# htsget-test-utils

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/action.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain

Common test functions and utilities used by [htsget-rs].

[htsget-rs]: https://github.com/umccr/htsget-rs

## Overview

This crate contains shared code used for testing by other htsget-rs crates. It has common server tests, as well as other
utility functions.

[noodles]: https://github.com/zaeleus/noodles

## Usage

### For running htsget-rs as an application

There is no need to interact with this crate for running htsget-rs.

### As a library

This crate contains `util` functions and `server_tests`. The `server_tests` use some example requests
to test the ticket server and the data block server. To use the `server_tests`, `TestServer` and
`TestRequest` need to be implemented, and then the `test_*` functions can be called.

This library is intended to be used as a [development dependency][dev-dependencies].

#### Feature flags

This crate has the following features:
* `s3-storage`: used to enable `AwsS3Storage` functionality.
* `server-tests`: used to enable server tests.

[value]: HERE
[dev-dependencies]: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#development-dependencies

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE