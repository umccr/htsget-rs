TBD: Architecture document that should follow a similar structure to [@matklad's writeup](https://matklad.github.io/2021/02/06/ARCHITECTURE.md.html)

TL;DR from the ARCHITECTURE blogpost:

* WHERE to change the code, given feature X, give pointers.
* Keep it short.
* What problem does this repo solve?
* Codemap: Where's the thing that does X, "map of the country, not an atlas"
* Avoid going into details of how each module works: separate docs and xrefs are better.
* Do name important files, modules, and types: NO LINKS (links go stale), JUST NAMES (symbol search).
* Call-out architectural invariants: explain absence of something.
* Point out boundaries between layers and systems.

* Example: https://github.com/rust-analyzer/rust-analyzer/blob/d7c99931d05e3723d878bea5dc26766791fa4e69/docs/dev/architecture.md

## Organization of the project

This repository consists of a workspace composed by the following crates:

- [htsget-search](htsget-search): Core logic needed to run searches in the genomic data according to the htsget specs: genomic data via reads variants from cloud storage, run queries on their indices. Other interfaces can be build outside of this crate but on top of this core functionality. 
- [htsget-http-core](htsget-http-core): Handling of htsget's HTTP requests: converting query results to JSON, client error reporting. Aims contain everything HTTP related that isn't framework dependent.
- [htsget-http-actix](htsget-http-actix): This crate contains a working server implementation based on the other crates in the project. It contains the framework dependent code. It should be possible for anyone to write another crate like this one using htsget-search, htsget-http-core and their preferred framework;
- [htsget-devtools](htsget-devtools): This is just a bunch of code helping us to explore the formats or to proof some concepts. Nothing to take very seriously ;-P

More crates will come as we progress in this project, such as the htsget id resolver interface layer.

# Architecture of htsget-search

This crate provides two basic abstractions:

- [htsget](htsget-search/src/htsget/mod.rs#L18): The `htsget` trait represents an entity that can resolve queries according to the htsget spec.
  The `htsget` trait comes together with a basic model to represent basic entities needed to perform a search (`Query`, `Format`, `Class`, `Tags`, `Headers`, `Url`, `Response`).
  We include a reference implementation called [htsgetFromStorage](htsget-search/src/htsget/from_storage.rs) that provides the logic to resolve queries using an external `Storage`.
  It can only [resolve queries for data in BAM format](htsget-search/src/htsget/bam_search.rs), but we [plan to support other formats](https://github.com/chris-zen/htsget-mvp/issues/7) too.

- [Storage](htsget-search/src/storage/mod.rs): The `Storage` trait represents some kind of object based storage (either locally or in the cloud) that can be used to retrieve files for alignments, variants or its respective indexes, as well as to get metadata from them. We include a reference implementation using [local files](htsget-search/src/storage/local.rs), but there are plans to [support AWS S3](https://github.com/chris-zen/htsget-mvp/issues/9) too.

## References

### HtsGet specification and references

[HtsGet specification](https://samtools.github.io/hts-specs/htsget.html)
[Google genomics HtsGet reference implementation](https://github.com/googlegenomics/htsget)

### SAM/BAM formats and tools

[SAM specification](https://github.com/samtools/hts-specs/blob/master/SAMv1.pdf)
[The great *noodles* library](https://github.com/zaeleus/noodles)
[Inspecting, summarizing, and manipulating the read alignments](https://mtbgenomicsworkshop.readthedocs.io/en/latest/material/day3/mappingstats.html)

### VCF/BCF formats

[VCF specification](https://samtools.github.io/hts-specs/VCFv4.3.pdf)

### Previous attempts to work on HtsGet with Rust

https://github.com/umccr/htsget-rs
https://github.com/brainstorm/htsget-indexer
https://github.com/brainstorm/bio-index-formats/

## Previous attempts to work on htsget with Rust

- https://github.com/umccr/htsget-rs
- https://github.com/brainstorm/htsget-indexer
- https://github.com/brainstorm/bio-index-formats/