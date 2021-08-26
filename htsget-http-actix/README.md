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

| Variable | Description | Default |
|---|---|---|
| HTSGET_IP| IP address | 127.0.0.1 |
| HTSGET_PORT| TCP Port | 8080 |
| HTSGET_PATH| The path to the directory where the server starts | `$PWD` | 
| HTSGET_REGEX| The regular expression an ID should match. | ".*" |
| HTSGET_REPLACEMENT| The replacement expression, to produce a key from an ID. | "$0" |
| HTSGET_ID| ID of the service. | "" |
| HTSGET_NAME| Name of the service. | HtsGet service |
| HTSGET_VERSION | Version of the service | ""
| HTSGET_ORGANIZATION_NAME| Name of the organization | Snake oil
| HTSGET_ORGANIZATION_URL| URL of the organization | https://en.wikipedia.org/wiki/Snake_oil |
| HTSGET_CONTACT_URL | URL to provide contact to the users | "" |
| HTSGET_DOCUMENTATION_URL| Link to documentation | https://github.com/umccr/htsget-rs/tree/main/htsget-http-actix |
| HTSGET_CREATED_AT | Date of the creation of the service. | "" |
| HTSGET_UPDATED_AT | Date of the last update of the service. | "" |
| HTSGET_ENVIRONMENT | Environment in which the service is running. | Testing |
For more information about the regex options look in the [documentation of the regex crate](https://docs.rs/regex/).

## Example cURL requests

As mentioned above, please keep in mind that the server will take the path where you executed it as base path to search for files. Here's a selection on what to ask this server for:

### GET

```shell
$ curl '127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer'
```

### POST

```shell
$ curl --header "Content-Type: application/json" -d '{}' '127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer'
```

### Parametrised GET

```shell
$ curl '127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer?format=VCF&class=header'
```

### Parametrised POST

```shell
$ curl --header "Content-Type: application/json" -d '{"format": "VCF", "regions": [{"referenceName": "chrM"}]}' '127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer'
```

### Service-info

```shell
$ curl 127.0.0.1:8080/variants/service-info
```

## Example Regular expressions
In this example 'data/' is added after the first '/'.
```shell
$ HTSGET_REGEX='(?P<group1>.*?)/(?P<group2>.*)' HTSGET_REPLACEMENT='$group1/data/$group2' cargo run --release -p htsget-http-actix
```
For more information about the regex options look in the [documentation of the regex crate](https://docs.rs/regex/).