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

## Quickstart

Launch a server instance:

```sh
cargo run -p htsget-axum
```

And fetch tickets from `localhost:8080`:

```sh
curl 'http://localhost:8080/variants/data/vcf/sample1-bcbio-cancer'
```

This crate uses [htsget-config] for configuration.

### Storage backends

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

By default, htsget-rs runs without TLS. To use TLS, pem formatted X.509 certificates are required.

For development and testing purposes, self-signed certificates can be used. For example, to generate self-signed certificates run:

```sh
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -sha256 -days 365 -nodes -subj '/CN=localhost'
```

It is not recommended to use self-signed certificates in a production environment as this is considered insecure.

There two server instances that are launched when running this crate, the ticket server and data block server. TLS
is specified separately for both servers.

#### Example requests

Using default configuration settings, this crate responds to queries referencing files in the [`data`][data] directory.
Some example requests using `curl` are shown below:

* GET

```sh
curl 'http://localhost:8080/variants/data/vcf/sample1-bcbio-cancer'
```

* POST

```sh
curl --header "Content-Type: application/json" -d '{}' 'http://localhost:8080/variants/data/vcf/sample1-bcbio-cancer'
```

* Parametrised GET

```sh
curl 'http://localhost:8080/variants/data/vcf/sample1-bcbio-cancer?format=VCF&class=header'
```

* Parametrised POST

```sh
curl --header "Content-Type: application/json" -d '{"format": "VCF", "regions": [{"referenceName": "chrM"}]}' 'http://localhost:8080/variants/data/vcf/sample1-bcbio-cancer'
```

* Service info

```sh
curl 'http://localhost:8080/variants/service-info'
```

### Crypt4GH

The htsget-rs server experimentally supports serving [Crypt4GH][c4gh] encrypted files to clients. See the [Crypt4GH section][config-c4gh]
in the configuration for more details on how to configure this.

To use Crypt4GH run the server using the [example config][example-config] and the `experimental` flag:

```sh
cargo run -p htsget-axum --features experimental -- --config htsget-config/examples/config-files/c4gh.toml
```

Crypt4GH encrypted byte ranges can be queried:

```sh
curl 'http://localhost:8080/reads/data/c4gh/htsnexus_test_NA12878?referenceName=11&start=5000000&end=5050000'
```

The output consists of the Crypt4GH header, which includes the original header, the edit lists, and the re-encrypted header that
the recipient can use to decrypt bytes:

```json
{
  "htsget": {
    "format": "BAM",
    "urls": [
      {
        "url": "data:;base64,Y3J5cHQ0Z2gBAAAAAwAAAA=="
      },
      {
        "url": "http://127.0.0.1:8081/data/c4gh/htsnexus_test_NA12878.bam.c4gh",
        "headers": {
          "Range": "bytes=16-123"
        }
      },
      {
        "url": "data:;base64,bAAAAAAAAABPIoRdk+d+ifp2PWRFeXoe6Z9kPOj+HrREhzxZ3QiDa2SYh+0Gy8aKpFic4MtTa+ywMpkHziJgojVbcmbvBAr3G7o01lDubsBW98aQ/U1AcalIUCp0fGNkrtdTBN4NaVNIdtQmbAAAAAAAAABPIoRdk+d+ifp2PWRFeXoe6Z9kPOj+HrREhzxZ3QiDa+xJ+yh+52zHvw8qQXMyCtqT6jTFvaYhRPw/6ZzvOdt98YPQgCcTIut58VeTGmR3ien0TdcQFxmfE10MH4qapF2blgjX"
      },
      {
        "url": "http://127.0.0.1:8081/data/c4gh/htsnexus_test_NA12878.bam.c4gh",
        "headers": {
          "Range": "bytes=124-1114711"
        }
      },
      {
        "url": "http://127.0.0.1:8081/data/c4gh/htsnexus_test_NA12878.bam.c4gh",
        "headers": {
          "Range": "bytes=2557120-2598042"
        }
      }
    ]
  }
}                       
```

For example, using a [htsget client][htsget-client], the data can be concatenated, and then decrypted using the [Crypt4GH CLI][crypt4gh-cli]:

```sh
htsget 'http://localhost:8080/reads/data/c4gh/htsnexus_test_NA12878?referenceName=11&start=5000000&end=5050000' > out.c4gh
crypt4gh decrypt --sk data/c4gh/keys/alice.sec < out.c4gh > out.bam
samtools view out.bam
```

### As a library

This crates has some components which may be useful to other crates. Namely, in contains Axum routing functions for
htsget-rs. It also contains the data block server which fetches data from a `LocalStorage` storage backend using [htsget-storage].

#### Feature flags

This crate has the following features:
* `s3-storage`: used to enable `S3Storage` functionality.
* `url-storage`: used to enable `UrlStorage` functionality.
* `experimental`: used to enable experimental features that aren't necessarily part of the htsget spec, such as Crypt4GH support through `C4GHStorage`.

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE
[config-c4gh]: ../htsget-config/README.md#crypt4gh
[data-c4gh]: ../data/c4gh
[c4gh]: https://samtools.github.io/hts-specs/crypt4gh.pdf
[htsget-client]: https://htsget.readthedocs.io/en/stable/installation.html
[crypt4gh-cli]: https://github.com/ega-archive/crypt4gh-rust
[example-config]: ../htsget-config/examples/config-files/c4gh.toml