# htsget-search

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/action.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain

Contains storage interfaces and abstractions for [htsget-rs]. It:
* Allows [htsget-rs] to interact with storage to fetch and retrieve bioinformatics files like indexes.
* Contains logic for local filesystem access, [AWS S3][s3-docs] cloud access and arbitrary URL server access.

[htsget-rs]: https://github.com/umccr/htsget-rs

## Overview

This crate is the mechanism htsget-rs uses to fetches data from the bioinformatics files it needs to
process requests. It also allows htsget-rs to create and format URL tickets correctly. It does this by providing storage
layer abstractions which other crates can use to interact with data. It defines the following storage layers:
* [local]: Access files on the local filesystem.
* [s3]: Access files on [AWS S3][s3-docs].
* [url]: Access files on any server which can respond to requests.
* [c4gh]: Access and process Crypt4GH-encrypted files.

[s3-docs]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html

This crate is responsible for allowing the user to fetch the URL tickets returned by the ticket server. With
`LocalStorage` a separate `data_server` is used to serve files using HTTP. `S3Storage` returns
presigned S3 URLs.

## Usage

In order to use a particular storage backend for URL tickets, the proper backend should be configured using [htsget-config].

[htsget-config]: ../htsget-config

### As a library

This crate provides have the following features:

* The `Storage` trait contains functions used to fetch data: `get`, `range_url`, `head` and `data_url`. The [local], [s3],
and [url] modules implement the `Storage` functionality.

#### Feature flags

This crate has the following features:
* `s3`: used to enable `S3` location functionality.
* `url`: used to enable `Url` location functionality.
* `experimental`: used to enable experimental features that aren't necessarily part of the htsget spec, such as Crypt4GH support through `C4GHStorage`.

[local]: src/local.rs
[s3]: src/s3.rs
[url]: src/url.rs
[c4gh]: src/c4gh/mod.rs

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE