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
Benchmarks for this crate written using [Criterion.rs][criterion-rs], and aim to compare the performance of this crate with the
[htsget Reference Server][htsget-refserver].
There are a set of light benchmarks, and one heavy benchmark. Light benchmarks can be performed by executing:

```
cargo bench -p htsget-axum -- LIGHT
```

To run the heavy benchmark, an additional vcf file needs to be downloaded, and placed in the [`data/vcf`][data-vcf] directory:

```
curl ftp://ftp.1000genomes.ebi.ac.uk/vol1/ftp/data_collections/1000_genomes_project/release/20190312_biallelic_SNV_and_INDEL/ALL.chr14.shapeit2_integrated_snvindels_v2a_27022019.GRCh38.phased.vcf.gz > data/vcf/internationalgenomesample.vcf.gz
```

Then to run the heavy benchmark:

```
cargo bench -p htsget-axum -- HEAVY
```

[criterion-rs]: https://github.com/bheisler/criterion.rs
[htsget-refserver]: https://github.com/ga4gh/htsget-refserver
[data-vcf]: ../data/vcf

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE