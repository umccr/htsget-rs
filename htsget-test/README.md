# htsget-test

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

## Usage

There is no need to interact with this crate for running htsget-rs.

### As a library

This crate contains `util` functions and `server_tests`. The `server_tests` use some example requests
to test the ticket server and the data block server. To use the `server_tests`, `TestServer` and
`TestRequest` need to be implemented, and then the `test_*` functions can be called.

This library is intended to be used as a [development dependency][dev-dependencies].

#### Feature flags

This crate has the following features:
* `http`: used to enable common functionality for HTTP tests.
* `aws-mocks`: used to enable AWS mocking for tests.
* `s3-storage`: used to enable `S3` location functionality.
* `url-storage`: used to enable `Url` location functionality.
* `experimental`: used to enable experimental features that aren't necessarily part of the htsget spec, such as Crypt4GH support through `C4GHStorage`.

[dev-dependencies]: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#development-dependencies

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE