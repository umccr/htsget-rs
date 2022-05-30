# htsget Rust server
This crate should allow to setup an [htsget](http://samtools.github.io/hts-specs/htsget.html) compliant server. For that purpose it uses the htsget-search, htsget-http-core and actix-web crates as dependencies.

## Quickstart 

These are some examples with [curl](https://github.com/curl/curl). **For the curl examples shown below to work, we assume that the server is being started from the root of the [htsget-rs project](https://github.com/umccr/htsget-rs)**, so we can use the example files inside the `data` directory.

To test them you can run:

```shell
$ cargo run -p htsget-http-actix
```

From **the top of the project**. Alternatively, the `HTSGET_PATH` environment variable can be set accordingly if the current working directory is `htsget-http-actix`, i.e:

```shell
$ HTSGET_PATH=../ cargo run
```

Otherwise we could have problems as [directory traversal](https://en.wikipedia.org/wiki/Directory_traversal_attack) isn't allowed.

## Environment variables 

There are reasonable defaults to allow the user to spin up the server as fast as possible, but all of them are configurable via environment variables.

Since this service can be used in serverless environments, no `dotenv` configuration is needed, [adjusting the environment variables below prevent accidental leakage of settings and sensitive information](https://medium.com/@softprops/configuration-envy-a09584386705).

| Variable                   | Description                                                                                                                              | Default          |
|----------------------------|------------------------------------------------------------------------------------------------------------------------------------------|------------------|
| HTSGET_ADDR                | The socket address for the server which creates response tickets.                                                                        | "127.0.0.1:8080" |
| HTSGET_PATH                | The path to the directory where the server starts                                                                                        | "."              | 
| HTSGET_REGEX               | The regular expression an ID should match.                                                                                               | ".*"             |
| HTSGET_SUBSTITUTION_STRING | The replacement expression, to produce a key from an ID.                                                                                 | "$0"             |
| HTSGET_STORAGE_TYPE        | Either "LocalStorage" or "AwsS3Storage", representing which storage type to use.                                                         | "LocalStorage"   |
| HTSGET_TICKET_SERVER_ADDR  | The socket address to use for the server which responds to tickets. Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".                 | "127.0.0.1:8081" |
| HTSGET_TICKET_SERVER_KEY   | The path to the PEM formatted X.509 private key used by the ticket response server. Unused if HTSGET_STORAGE_TYPE is not "LocalStorage". | "key.pem"        |
| HTSGET_TICKET_SERVER_CERT  | The path to the PEM formatted X.509 certificate used by the ticket response server. Unused if HTSGET_STORAGE_TYPE is not "LocalStorage". | "cert.pem"       |
| HTSGET_S3_BUCKET           | The name of the AWS S3 bucket. Unused if HTSGET_STORAGE_TYPE is not "AwsS3Storage".                                                      | ""               |
| HTSGET_ID                  | ID of the service.                                                                                                                       | "None"           |
| HTSGET_NAME                | Name of the service.                                                                                                                     | "None"           |
| HTSGET_VERSION             | Version of the service.                                                                                                                  | "None"           |
| HTSGET_ORGANIZATION_NAME   | Name of the organization.                                                                                                                | "None"           |
| HTSGET_ORGANIZATION_URL    | URL of the organization.                                                                                                                 | "None"           |
| HTSGET_CONTACT_URL         | URL to provide contact to the users.                                                                                                     | "None"           |
| HTSGET_DOCUMENTATION_URL   | Link to documentation.                                                                                                                   | "None"           |
| HTSGET_CREATED_AT          | Date of the creation of the service.                                                                                                     | "None"           |
| HTSGET_UPDATED_AT          | Date of the last update of the service.                                                                                                  | "None"           |
| HTSGET_ENVIRONMENT         | Environment in which the service is running.                                                                                             | "None"           |
For more information about the regex options look in the [documentation of the regex crate](https://docs.rs/regex/).

## Example cURL requests

As mentioned above, please keep in mind that the server will take the path where you executed it as base path to search for files. Here's a selection on what to ask this server for:

### GET

```shell
$ curl '127.0.0.1:8080/variants/vcf/sample1-bcbio-cancer'
```

### POST

```shell
$ curl --header "Content-Type: application/json" -d '{}' '127.0.0.1:8080/variants/vcf/sample1-bcbio-cancer'
```

### Parametrised GET

```shell
$ curl '127.0.0.1:8080/variants/vcf/sample1-bcbio-cancer?format=VCF&class=header'
```

### Parametrised POST

```shell
$ curl --header "Content-Type: application/json" -d '{"format": "VCF", "regions": [{"referenceName": "chrM"}]}' '127.0.0.1:8080/variants/vcf/sample1-bcbio-cancer'
```

### Service-info

```shell
$ curl 127.0.0.1:8080/variants/service-info
```

## Running the benchmarks
There are benchmarks for the htsget-search crate and for the htsget-http-actix crate. The first ones work like normal benchmarks, but the latter ones try to compare the performance of this implementation and the [reference implementation](https://github.com/ga4gh/htsget-refserver).
There are a set of light benchmarks, and one heavy benchmark. Light benchmarks can be performed by executing:

```
cargo bench -p htsget-http-actix -- LIGHT
```

In order to run the heavy benchmark, an additional vcf file should be downloaded, and placed in the `data/vcf` directory:

```
curl ftp://ftp.1000genomes.ebi.ac.uk/vol1/ftp/data_collections/1000_genomes_project/release/20190312_biallelic_SNV_and_INDEL/ALL.chr14.shapeit2_integrated_snvindels_v2a_27022019.GRCh38.phased.vcf.gz > data/vcf/internationalgenomesample.vcf.gz
```

Then to run the heavy benchmark:

```
cargo bench -p htsget-http-actix -- HEAVY
```

## Example Regular expressions
In this example 'data/' is added after the first '/'.
```shell
$ HTSGET_REGEX='(?P<group1>.*?)/(?P<group2>.*)' HTSGET_SUBSTITUTION_STRING='$group1/data/$group2' cargo run --release -p htsget-http-actix
```
For more information about the regex options look in the [documentation of the regex crate](https://docs.rs/regex/).