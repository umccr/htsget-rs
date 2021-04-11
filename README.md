# HtsGet implementation for Rust

For now, the aim of this project is to build a Minimal Viable Product (MVP) for a HtsGet API with Rust.

## Organization of the project

This repository consists of a workspace composed by the following crates:

- [htsget-search](htsget-search): This crate will contain the core logic needed to run searches in the genomic data according to the HtsGet specs. Things like how to access the genomic data with the reads or the variants from cloud storage, or how to run queries on their indices, or how to provide the information needed by the HtsGet spec. But this crate has nothing to do with the HTTP or REST protocols, only the core logic, which means that it could also be used to build other kind of interfaces on top of it, like Command Line Interfaces (CLI) for example.
- [htsget-devtools](htsget-devtools): This is just a bunch of code helping us to explore the formats or to proof some concepts. Nothing to take very seriously ;-P

More crates will come as we progress in this project, for example the HTTP/REST layer.

## Architecture of htsget-search

This crate provides two basic abstractions:

- [HtsGet](htsget-search/src/htsget/mod.rs#L18): The `HtsGet` trait represents an entity that can resolve queries according to the HtsGet spec.
  The `HtsGet` trait comes together with a basic model to represent basic entities needed to perform a search (`Query`, `Format`, `Class`, `Tags`, `Headers`, `Url`, `Response`).
  We include a reference implementation called [HtsGetFromStorage](htsget-search/src/htsget/from_storage.rs) that provides the logic to resolve queries using an external `Storage`.
  It can only [resolve queries for data in BAM format](htsget-search/src/htsget/bam_search.rs), but we [plan to support other formats](https://github.com/chris-zen/htsget-mvp/issues/7) too.

- [Storage](htsget-search/src/storage/mod.rs): The `Storage` trait represents some kind of object based storage (either locally or in the cloud) that can be used to retrieve files for alignments, variants or its respective indexes, as well as to get metadata from them. We include a reference implementation using [local files](htsget-search/src/storage/local.rs), but there are plans to [support AWS S3](https://github.com/chris-zen/htsget-mvp/issues/9) too.

## References

### HtsGet specification and references

- [HtsGet specification](https://samtools.github.io/hts-specs/htsget.html)
- [Google genomics HtsGet reference implementation](https://github.com/googlegenomics/htsget)

### SAM/BAM formats and tools

- [SAM specification](https://github.com/samtools/hts-specs/blob/master/SAMv1.pdf)
- [The great *noodles* library](https://github.com/zaeleus/noodles)
- [Inspecting, summarizing, and manipulating the read alignments](https://mtbgenomicsworkshop.readthedocs.io/en/latest/material/day3/mappingstats.html)

### Previous attempts to work on HtsGet with Rust

- https://github.com/umccr/htsget-rs
- https://github.com/brainstorm/htsget-indexer
- https://github.com/brainstorm/bio-index-formats/

## Google Summer of Code 2021

This project participates in the [GSoC for 2021](https://summerofcode.withgoogle.com/organizations/5907083486035968/) under the [Global Alliance for Genomics and Health](https://www.ga4gh.org/). If you are interested on participating, please apply for the [idea "Pure Rust serverless htsget implementation" in this document](https://docs.google.com/document/d/1Ep7aoOuQD2B5pWCG_bVANb8JVHZ2SoNDa9BJARhv_e0/edit#heading=h.vjm3s4ho0ys) contacting the primary mentor.

## License

This project is distributed under the terms of the MIT license.

See [LICENSE](LICENSE) for details.