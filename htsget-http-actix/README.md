# htsget-http-actix

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/action.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain

Framework dependent code for a local instance of [htsget-rs], using [Actix Web][actix-web].

[htsget-rs]: ../
[actix-web]: https://actix.rs/

## Overview

This crate is used for running a local instance of htsget-rs. It is based on:
* [Actix Web][actix-web] for endpoints, routes, and middleware.
* [htsget-http-core] for htsget-rs specific HTTP responses

[htsget-http-core]: ../htsget-http-core

## Usage

### For running htsget-rs as an application

This crate uses [htsget-config] for configuration. See [htsget-config] for details on how to configure this crate.

To run an instance of this crate, execute the following command:
```shell
cargo run -p htsget-http-actix
```
Using the default configuration, this will start a ticket server on `127.0.0.1:8080` and a data block server on `127.0.0.1:8081`
with data accessible from the [`data`][data] directory.

If only `LocalStorage` is required, compiling code related `AwsS3Storage` can be avoided by running the following:

```shell
cargo run -p htsget-http-actix --no-default-features
```

See [htsget-search] for details on how to structure files.

[htsget-config]: ../htsget-config
[data]: ../data
[htsget-search]: ../htsget-search

#### Using TLS

There two server instances that are launched when running this crate. The ticket server, which returns a list of ticket URLs that a client must fetch.
And the data block server, which responds to the URLs in the tickets. By default, the data block server runs without TLS. 
To run the data block server with TLS, pem formatted X.509 certificates are required.

For development and testing purposes, self-signed certificates can be used.
For example, to generate self-signed certificates run:

```shell
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -sha256 -days 365 -nodes -subj '/CN=localhost'
```

It is not recommended to use self-signed certificates in a production environment 
as this is considered insecure.

#### Example requests

Using default configuration settings, this crate responds to queries referencing files in the [`data`][data] directory.
Some example requests using `curl` are shown below:

* GET

```shell
curl '127.0.0.1:8080/variants/vcf/sample1-bcbio-cancer'
```

* POST

```shell
curl --header "Content-Type: application/json" -d '{}' '127.0.0.1:8080/variants/vcf/sample1-bcbio-cancer'
```

* Parametrised GET

```shell
curl '127.0.0.1:8080/variants/vcf/sample1-bcbio-cancer?format=VCF&class=header'
```

* Parametrised POST

```shell
curl --header "Content-Type: application/json" -d '{"format": "VCF", "regions": [{"referenceName": "chrM"}]}' '127.0.0.1:8080/variants/vcf/sample1-bcbio-cancer'
```

* Service info

```shell
curl '127.0.0.1:8080/variants/service-info'
```

### As a library

There shouldn't be any need to interact with this crate
as a library, however some functions which deal with configuring routes 
are exposed in the public API.

#### Feature flags

This crate has the following features:
* `s3-storage`: used to enable `AwsS3Storage` functionality.

## Benchmarks
Benchmarks for this crate written using [Criterion.rs][criterion-rs], and aim to compare the performance of this crate with the 
[htsget Reference Server][htsget-refserver].
There are a set of light benchmarks, and one heavy benchmark. Light benchmarks can be performed by executing:

```
cargo bench -p htsget-http-actix -- LIGHT
```

To run the heavy benchmark, an additional vcf file needs to be downloaded, and placed in the [`data/vcf`][data-vcf] directory:

```
curl ftp://ftp.1000genomes.ebi.ac.uk/vol1/ftp/data_collections/1000_genomes_project/release/20190312_biallelic_SNV_and_INDEL/ALL.chr14.shapeit2_integrated_snvindels_v2a_27022019.GRCh38.phased.vcf.gz > data/vcf/internationalgenomesample.vcf.gz
```

Then to run the heavy benchmark:

```
cargo bench -p htsget-http-actix -- HEAVY
```

[criterion-rs]: https://github.com/bheisler/criterion.rs
[htsget-refserver]: https://github.com/ga4gh/htsget-refserver
[data-vcf]: ../data/vcf

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE