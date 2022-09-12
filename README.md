![build](https://github.com/umccr/htsget-rs/actions/workflows/action.yml/badge.svg)

[![Logo](doc/img/ga4gh-logo.svg)](https://ga4gh.org)

# htsget-rs

A Rust **server** implementation of the [htsget protocol][htsget-spec].

## Quickstart

Htsget-rs is ready hit the ground running locally or deployed to commercial cloud providers such as Amazon Web Services.

### Local
Instantiating a demo htsget-rs server is as simple as running:

```
$ cargo run -p htsget-http-actix
```

This uses the default settings with example files from the `data` directory and the server listening to requests on port 8080. Please refer to the [htsget-http-actix crate README.md for further details][htsget-http-actix-readme].

### Cloud

To deploy to an AWS account, please refer to the `deploy/README.md` for further instructions.

## Intro

Htsget makes bioinformatic data formats accessible through HTTP in a consistent way.

This repo implements a 100% Rust implementation of the [htsget spec][htsget-spec] using [Noodles][noodles] which gets rid of the [`unsafe` interfacing][rust-htslib] with the C-based [htslib](https://github.com/samtools/htslib).

Our Rust implementation distinguishes itself from others in the following ways:

|          	| [htsnexus][dnanexus] 	| [google][google-htsget] | [ga4gh][ga4gh-ref] | [EBI][ebi-htsget] | [gel-htsget][gel-htsget] | [htsget-rs][htsget-rs] | [CanDIG][candig-htsget]
|---	    	  |---      | ---                |  ---	 |  ---	  | --- |	---             |   ---   |
| maintained[1]   | ❌      | ❌ 	                | ✅    |  ❌    | ✅  |  ✅                |   ✅    |
| local           | ✅      | ❌ 	                | ✅	   |  ✅	   | ✅ |   ✅                |   ✅    |
| serverless      | ❌      | ❌	                | ❌    |  ❌    | ❌ |     ✅|   ❌    |
| BAM             | ✅      | ✅ 	                | ✅    |  ✅    | ✅ |   ✅                |   ✅    |
| CRAM            | ✅	   | ❌ 	                | ✅    |  ✅    | ✅ |   ✅                |   ✅    |
| VCF             | ✅	   | [❌][google-novcf]  | ✅    |  ✅    | ✅ |   ✅                |   ✅    |
| BCF             | ✅	   | ✅  	            | ✅    |  ✅    | ✅ |   ✅                |   ✅    |
| storage[2]      | ❌      | ❌  	            | ❌    |  ❌    | ❌ |   ✅                |   ❌    |
| [safe][safe-unsafe] | ❌  | ❌                  | ❌    |  ❌    | ❌ |   ✅                |   ❌    |
| testsuite         |  ❌     | ❌                  | ✅    |   ✅    |  ✅ |   ✅    |    ✅    |
| benchmarks      |  ❌     | ❌                  | ❌    |  ❌    | ❌ |   [🚧][benches]     |   ❌    |
| language        | C++     | Go                 | Go    |  Perl  | Python |  Rust          | Python  |

Hover over some of the tick marks for a reference of the issues 👆 Regarding some of the criteria annotations in the table:

1. No signs of activity in main repository in >6 months. Maintainers: [please open an issue if that's not the case or the repo has been relocated and/or deprecated](https://github.com/umccr/htsget-rs/issues/new).
1. Decoupled (relatively easy to exchange) storage backends.

[ebi-htsget]: https://github.com/andrewyatz/basic-htsget
[gel-htsget]: https://gitlab.com/genomicsengland/htsget/gel-htsget
[htsget-rs]: https://github.com/umccr/htsget-rs
[dnanexus]: https://github.com/dnanexus-rnd/htsnexus
[google-htsget]: https://github.com/googlegenomics/htsget
[google-novcf]: https://github.com/googlegenomics/htsget/issues/34
[ga4gh-ref]: https://github.com/ga4gh/htsget-refserver
[candig-htsget]: https://github.com/CanDIG/htsget_app
[aws-fixing]: https://github.com/umccr/htsget-rs/issues/47
[benches]: https://github.com/umccr/htsget-rs/pull/59
[safe-unsafe]: https://doc.rust-lang.org/nomicon/meet-safe-and-unsafe.html

## Architecture

Please refer to [the architecture of this project](doc/ARCHITECTURE.md) to get a grasp of how this project is structured and how to contribute if you'd like so :)

## License

This project is distributed under the terms of the MIT license.

See [LICENSE](LICENSE) for details.

[noodles]: https://github.com/zaeleus/noodles
[htsget-spec]: https://samtools.github.io/hts-specs/htsget.html
[rust-htslib]: https://github.com/rust-bio/rust-htslib
[htsget-http-actix-readme]: https://github.com/umccr/htsget-rs/blob/main/htsget-http-actix/README.md
