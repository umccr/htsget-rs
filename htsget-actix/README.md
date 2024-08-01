# htsget-actix

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/action.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain

> [!IMPORTANT]  
> The functionality of [htsget-axum] is identical to this crate and it is recommended for all
> projects to use [htsget-axum] instead.
> 
> This crate will be maintained to preserve backwards compatibility, however [htsget-axum] is
> favoured because it contains components that better fit with the rest of htsget-rs, and [htsget-actix]
> depends on some of them.

Framework dependent code for a server instance of [htsget-rs], using [Actix Web][actix-web].

[htsget-rs]: https://github.com/umccr/htsget-rs
[actix-web]: https://actix.rs/
[htsget-actix]: .

## Overview

This crate is used for running a local instance of htsget-rs. It is based on:
* [Actix Web][actix-web] for endpoints, routes, and middleware.
* [htsget-http] for htsget-rs specific HTTP responses

[htsget-http]: ../htsget-http

## Usage

This application has the same functionality as [htsget-axum]. To use it, following the [htsget-axum][htsget-axum-usage] instructions, and
replace any calls to `htsget-axum` with `htsget-actix`.

### As a library

There shouldn't be any need to interact with this crate as a library, however some functions which deal with configuring routes 
are exposed in the public API.

#### Feature flags

This crate has the following features:
* `s3-storage`: used to enable `S3Storage` functionality.
* `url-storage`: used to enable `UrlStorage` functionality.

## Benchmarks

There are a set of benchmarks for this crate which are similar to those for [htsget-axum]. They also use  [Criterion.rs][criterion-rs],
and aim to compare the performance of this crate with the [htsget Reference Server][htsget-refserver]. To run benchmarks
follow the benchmarks instructions for [htsget-axum][htsget-axum-bench], replacing calls to `htsget-axum` with `htsget-actix`.

[criterion-rs]: https://github.com/bheisler/criterion.rs
[htsget-refserver]: https://github.com/ga4gh/htsget-refserver
[htsget-axum]: ../htsget-axum
[htsget-axum-usage]: ../htsget-axum#usage
[htsget-axum-bench]: ../htsget-axum#benchmarks

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE