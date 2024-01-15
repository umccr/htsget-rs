# htsget-search

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/action.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain

Creates URL tickets for [htsget-rs] by processing bioinformatics files. It:
* Takes a htsget query and produces htsget URL tickets.
* Uses [noodles] to process files.
* Features a storage abstraction layer which can represent data locally or in the cloud.

[htsget-rs]: https://github.com/umccr/htsget-rs

## Overview

This crate is the primary mechanism by which htsget-rs interacts with, and processes
bioinformatics files. It does this by using [noodles] to query files and their indices.
It is split up into two modules:
* [htsget]: which contains abstractions that remove commonalities between file formats. Together with file format 
specific code, this defines an interface that handles the core logic of a htsget request.
* [storage]: which implements an object based storage abstraction, either locally or on the cloud, that can be used to fetch data. 

Future work may split these two modules into separate crates.

There are three different kinds of storage:
* `LocalStorage`: which spawns a local server that can respond to URL tickets.
* `S3Storage`: which returns pre-signed AWS S3 URLs for the tickets.
* `UrlStorage`: which returns a custom URL endpoint which is intended to respond to URL tickets.
    * For `UrlStorage`, returning Crypt4GH encrypted files is supported using a custom protocol,
      by compiling with the `crypt4gh` flag. See the crypt4gh [ARCHITECTURE.md][architecture] file for Crypt4GH for a description on
      how this works.

[noodles]: https://github.com/zaeleus/noodles
[architecture]: ../docs/crypt4gh/ARCHITECTURE.md

### Traits abstraction

The two modules are architectured to remove commonalities between file formats and to allow implementing additional features with ease.
The `storage` module is the location of storage backends. This module acts as the 'data server', as 
described by the htsget protocol, and implementing an additional backend requires implementing the `Storage` trait. This trait is used 
by `htsget` to fetch the underlying file and query the data. For example, similar to `S3Storage`, a Cloudflare R2 storage
could be added. 

Note that the storage backend is responsible for allowing the user to fetch the URL tickets returned by the
ticket server. In the case of `LocalStorage`, this entails a separate `data_server` that can serve files using HTTP. `S3Storage`
simply returns presigned S3 URLs.

## Usage

### For running htsget-rs as an application

This crate is responsible for handling bioinformatics file data. It supports BAM, CRAM, VCF and BCF files.
For htsget-rs to function, files need to be organised in the following way:

* Each file format is paired with an index. All files must have specific extensions.
    * BAM: File must end with `.bam`; paired with BAI index, which must end with `.bam.bai`.
    * CRAM: File must end with `.cram`; paired with CRAI index, which must end with `.cram.crai`.
    * VCF: File must end with `.vcf.gz`; paired with TBI index, which must end with `.vcf.gz.tbi`.
    * BCF: File must end with `.bcf`; paired with CSI index, which must end with `.bcf.csi`.
* VCF files are assumed to be BGZF compressed.
* BGZF compressed files (BAM, CRAM, VCF) can optionally also have a [GZ index][gzi] to make byte ranges smaller.
    * GZI files must end with `.gzi`.
    * See [minimising byte ranges][minimising-byte-ranges] for more details on GZI.
* Crypt4GH encrypted files must end with `.c4gh`.

This is quite inflexible, and is likely to change in the future to allow arbitrary mappings of files and indices.

[gzi]: http://www.htslib.org/doc/bgzip.html#GZI_FORMAT
[minimising-byte-ranges]: #minimising-byte-ranges

### As a library

The two modules that this crate provides have the following features:

* [htsget]: The `HtsGet` trait represents an entity that can resolve queries according to the htsget spec. 
The htsget trait comes with a basic model to represent components needed to perform a search: `Query`, `Format`, 
`Class`, `Tags`, `Headers`, `Url`, `Response`. `HtsGetFromStorage` is the struct which is 
used to process requests.
* [storage]: The `Storage` trait contains functions used to fetch data: `get`, `range_url`, `head` and `data_url`.

#### Feature flags

This crate has the following features:
* `s3-storage`: used to enable `S3Storage` functionality.
* `url-storage`: used to enable `UrlStorage` functionality.
* `crypt4gh`: used to enable Crypt4GH functionality.

[htsget]: src/htsget
[storage]: src/storage

## Minimising Byte Ranges

One challenge involved with implementing htsget is meaningfully minimising the size of byte ranges returned in response
tickets. Since htsget is used to reduce the amount of data a client needs to fetch by querying specific parts of a file, 
the data returned by htsget should ideally be as minimal as possible. This is done by reading the index file or
the underlying target file, to determine the required byte ranges. However, this is complicated when considering 
BGZF compressed files. 

For BGZF compressed files, htsget-rs needs to return compressed byte positions. Also, after concatenating data from URL tickets,
the resulting file must be valid. This means that byte ranges must start and finish on BGZF blocks, otherwise the concatenation
would not result in a valid file. However, index files (BAI, TBI, CSI) do not contain all the information required to
produce minimal byte ranges. For example, consider this [file][example-file]:
* There are 14 BGZF blocks positions using all available data in the corresponding [index file][example-index] (chunk start positions, chunk end positions, linear index positions, and metadata positions):
    * `4668`, `256721`, `499249`, `555224`, `627987`, `824361`, `977196`, `1065952`, `1350270`, `1454565`, `1590681`, `1912645`, `2060795` and `2112141`.
* Using just this data, the following query with: 
  * `referenceName=11`, `start=5015000`, and `end=5050000`
* Would produce these byte ranges:
  * `bytes=0-4667`
  * `bytes=256721-1065951`
* However, an equally valid response, with smaller byte ranges is:
  * `bytes=0-4667`
  * `bytes=256721-647345`
  * `bytes=824361-842100`
  * `bytes=977196-996014`

To produce the smallest byte ranges, htsget-rs needs to find this data somewhere else. There are two ways to accomplish this:
* Get the data from the underlying target file, by seeking to the start of a BGZF, and reading until the end of the block is found.
* Get the data from an auxiliary index file, such as GZI.

Currently, htsget-rs takes the latter approach, and uses GZI files, which contain information on all BGZF start and
end positions. However, this is not ideal, as GZI contains more information than required by htsget-rs. The former
approach also has issues when considering cloud-based storage, which in the case of S3, does not have seek operations.

The way htsget-rs finds the information needed for minimal byte ranges is very likely to change in the future, as more efficient
approaches are implemented. For example, a database could be used to further index files. Queries to a database could be
as targeted as possible, retrieving only the required information.

[example-file]: ../data/bam/htsnexus_test_NA12878.bam
[example-index]: ../data/bam/htsnexus_test_NA12878.bam.bai

## Benchmarks 

Since this crate is used to query file data, it is the most performance critical component of htsget-rs. Benchmarks, using 
[Criterion.rs][criterion-rs], are therefore written to test performance. Run benchmarks by executing:

```sh
cargo bench -p htsget-search --all-features
```

Alternatively if you are using `cargo-criterion` and want a machine readable JSON output, run:

```sh
cargo criterion --bench search-benchmarks --message-format=json -- LIGHT 1> search-benchmarks.json
```

[criterion-rs]: https://github.com/bheisler/criterion.rs

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE