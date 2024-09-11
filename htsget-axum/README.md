# htsget-axum

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/action.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain

Framework dependent code for a local instance of [htsget-rs], using [Axum][axum].

[htsget-rs]: https://github.com/umccr/htsget-rs
[axum]: https://github.com/tokio-rs/axum

## Overview

This crate is used for running a server instance of htsget-rs. It is based on:
* [Axum][axum] for endpoints, routes, and middleware.
* [htsget-http] for htsget-rs specific HTTP responses

[htsget-http]: ../htsget-http

## Usage

### For running htsget-rs as an application

This crate uses [htsget-config] for configuration. See [htsget-config] for details on how to configure this crate.

To run an instance of this crate, execute the following command:
```sh
cargo run -p htsget-axum
```
Using the default configuration, this will start a ticket server on `127.0.0.1:8080` and a data block server on `127.0.0.1:8081`
with data accessible from the [`data`][data] directory. This application supports storage backends defined in [htsget-storage].

To use `S3Storage`, compile with the `s3-storage` feature:
```sh
cargo run -p htsget-axum --features s3-storage
```
This will start a ticket server with `S3Storage` using a bucket called `"data"`.

To use `UrlStorage`, compile with the `url-storage` feature.

See [htsget-search] for details on how to structure files.

[htsget-config]: ../htsget-config
[data]: ../data
[htsget-search]: ../htsget-search
[htsget-storage]: ../htsget-storage

#### Using TLS

There two server instances that are launched when running this crate. The ticket server, which returns a list of ticket URLs that a client must fetch.
And the data block server, which responds to the URLs in the tickets. By default, the data block server runs without TLS. 
To run the data block server with TLS, pem formatted X.509 certificates are required.

For development and testing purposes, self-signed certificates can be used.
For example, to generate self-signed certificates run:

```sh
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -sha256 -days 365 -nodes -subj '/CN=localhost'
```

It is not recommended to use self-signed certificates in a production environment 
as this is considered insecure.

#### Example requests

Using default configuration settings, this crate responds to queries referencing files in the [`data`][data] directory.
Some example requests using `curl` are shown below:

* GET

```sh
curl '127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer'
```

* POST

```sh
curl --header "Content-Type: application/json" -d '{}' '127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer'
```

* Parametrised GET

```sh
curl '127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer?format=VCF&class=header'
```

* Parametrised POST

```sh
curl --header "Content-Type: application/json" -d '{"format": "VCF", "regions": [{"referenceName": "chrM"}]}' '127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer'
```

* Service info

```sh
curl '127.0.0.1:8080/variants/service-info'
```

### As a library

This crates has some components which may be useful to other crates. Namely, in contains Axum routing functions for
htsget-rs. It also contains the data block server which fetches data from a `LocalStorage` storage backend using [htsget-storage].

#### Feature flags

This crate has the following features:
* `s3-storage`: used to enable `S3Storage` functionality.
* `url-storage`: used to enable `UrlStorage` functionality.
* `c4gh-experimental`: used to enable `C4GHStorage` functionality.

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE