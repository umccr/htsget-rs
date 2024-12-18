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

[htsget-rs]: https://github.com/umccr/htsget-rs

## Overview

This crate is the primary mechanism by which htsget-rs interacts with, and processes
bioinformatics files. It does this by using [noodles] to query files and their indices.
This crate contains abstractions that remove commonalities between file formats. Together with file format 
specific code, this defines an interface that handles the core logic of a htsget request.

[noodles]: https://github.com/zaeleus/noodles

## File structure

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

[gzi]: http://www.htslib.org/doc/bgzip.html#GZI_FORMAT
[minimising-byte-ranges]: #minimising-byte-ranges

### As a library

This crate has the following features:

* The `HtsGet` trait represents an entity that can resolve queries according to the htsget spec. 
The htsget trait comes with a basic model to represent components needed to perform a search: `Query`, `Format`, 
`Class`, `Tags`, `Headers`, `Url`, `Response`. `HtsGetFromStorage` is the struct which is 
used to process requests.

#### Feature flags

This crate has the following features:
* `s3-storage`: used to enable `S3` location functionality.
* `url-storage`: used to enable `Url` location functionality.
* `experimental`: used to enable experimental features that aren't necessarily part of the htsget spec, such as Crypt4GH support through `C4GHStorage`.

## Minimising Byte Ranges

One challenge involved with implementing htsget is  minimising the size of byte ranges returned in response
tickets. Since htsget is used to reduce the amount of data a client needs to fetch by querying specific parts of a file, 
the data returned by htsget should ideally be as minimal as possible. This is done by reading the index file or
the underlying target file, to determine the required byte ranges.

For BGZF files, [GZI][gzi] files are supported, which enable the smallest possible byte ranges.

### BGZF file example

For BGZF compressed files, htsget-rs needs to return compressed byte positions. Also, after concatenating data from URL tickets,
the resulting file must be valid. This means that byte ranges must start and finish on BGZF blocks, otherwise the concatenation
would not result in a valid file. Index files (BAI, TBI, CSI) do not contain all the information required to
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

To produce the smallest byte ranges, htsget-rs needs can search through GZI files and regular index files. It does not
read data from the underlying target file.

[example-file]: ../data/bam/htsnexus_test_NA12878.bam
[example-index]: ../data/bam/htsnexus_test_NA12878.bam.bai

## Benchmarks

Since this crate is used to query file data, it is the most performance critical component of htsget-rs. Benchmarks, using 
[Criterion.rs][criterion-rs] are written to test performance. Run benchmarks by executing:

```sh
cargo bench -p htsget-search --all-features
```

Alternatively if you are using `cargo-criterion` and want a machine-readable JSON output, run:

```sh
cargo criterion --bench search-benchmarks --message-format=json -- LIGHT 1> search-benchmarks.json
```

[criterion-rs]: https://github.com/bheisler/criterion.rs

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE