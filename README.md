# htsget-rs

## Quickstart

Instantiating a demo htsget-rs server is as simple as running:

```
$ cargo run -p htsget-http-actix
```

Then the server is ready to listen to your requests on port 8080, please refer to the [htsget-http-actix crate README.md for furhter details][htsget-http-actix-readme].

## Intro
htsget makes bioinformatic data formats accessible through HTTP in a consistent way.

This repo implements a 100% Rust implementation of the [htsget spec][htsget-spec] using [Noodles][noodles].

Other implementation shortcomings have been identified and addressed, both in feature set and fundamental abstractions such as decoupled storage backends:

|          	| [htsnexus][dnanexus] 	| [google][google-htsget] | [ga4gh][ga4gh-ref] | [EBI][ebi-htsget] | [htsget-rs][htsget-rs]
|---	    	  |---	    | ---    |  ---	 |  ---	  | ---	   |
| maintained  | ❌ 	   | ❌ 	    | ✅	 	 |   ❌    |  ✅	  |
| local       | ✅	     | ❌ 	    | ✅	   |  ✅	    | ✅  |
| cloud       | ✅      | ✅ 	    | ✅   |  	 ❌  	|   🚧  |
| BAM         | ✅	     | ✅ 	    | ✅   |  	 ✅  |   ✅  |
| CRAM        | ✅	     | ❌ 	    | ✅ 	|  	  ✅ |   ✅  |
| VCF         | ✅	     | [❌][google-novcf]  | ✅   |  ✅      |  ✅   |
| BCF         | ✅	     | ✅  	     | ✅   |   ✅   |   ✅   |
| storage    | ❌      | ❌  	     | ❌    |    ❌     |   ✅  |
| htslib-free | ❌      | ❌         |  ❌ |  ❌      |   ✅  |
| rust | ❌      | ❌         |  ❌ |  ❌      |   ✅  |

[ebi-htsget]: https://github.com/andrewyatz/basic-htsget
[htsget-rs]: https://github.com/umccr/htsget-rs
[dnanexus]: https://github.com/dnanexus-rnd/htsnexus
[google-htsget]: https://github.com/googlegenomics/htsget
[google-novcf]: https://github.com/googlegenomics/htsget/issues/34
[ga4gh-ref]: https://github.com/ga4gh/htsget-refserver

## Architecture

Please refer to [the architecture of this project](ARCHITECTURE.md) to get a grasp of how this project is structured and how to contribute if you'd like so :)

## License

This project is distributed under the terms of the MIT license.

See [LICENSE](LICENSE) for details.

[htsget-spec]: https://samtools.github.io/hts-specs/htsget.html
[noodles]: https://github.com/zaeleus/noodles
[htsget-http-actix-readme]: https://github.com/umccr/htsget-rs/blob/main/htsget-http-actix/README.md