# htsget-config

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/action.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain

Configuration for [htsget-rs] and relevant crates.

[htsget-rs]: https://github.com/umccr/htsget-rs

## Overview

This crate is used to configure htsget-rs by reading environment variables.

There are plans to support config files to aid with more complex configuration in the future.

## Usage

### For running htsget-rs as an application

The following are environment variables that can be set to configure htsget-rs:

| Variable                               | Description                                                                                                                     | Default                 |
|----------------------------------------|---------------------------------------------------------------------------------------------------------------------------------|-------------------------|
| HTSGET_PATH                            | The path to the directory where the server starts                                                                               | "data"                  | 
| HTSGET_REGEX                           | The regular expression an ID should match.                                                                                      | ".*"                    |
| HTSGET_SUBSTITUTION_STRING             | The replacement expression, to produce a key from an ID.                                                                        | "$0"                    |
| HTSGET_STORAGE_TYPE                    | Either "LocalStorage" or "AwsS3Storage", representing which storage backend to use.                                             | "LocalStorage"          |
| HTSGET_TICKET_SERVER_ADDR              | The socket address for the server which creates response tickets.                                                               | "127.0.0.1:8080"        |
| HTSGET_TICKET_SERVER_ALLOW_CREDENTIALS | Boolean flag, indicating whether authenticated requests are allowed by including the `Access-Control-Allow-Credentials` header. | "false"                 |
| HTSGET_TICKET_SERVER_ALLOW_ORIGIN      | Which origin is allowed in the `ORIGIN` header.                                                                                 | "http://localhost:8080" |
| HTSGET_DATA_SERVER_ADDR                | The socket address to use for the server which responds to tickets. Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".        | "127.0.0.1:8081"        |
| HTSGET_DATA_SERVER_KEY                 | The path to the PEM formatted X.509 private key used by the data server. Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".   | "None"                  |
| HTSGET_DATA_SERVER_CERT                | The path to the PEM formatted X.509 certificate used by the data server. Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".   | "None"                  |
| HTSGET_DATA_SERVER_ALLOW_CREDENTIALS   | Boolean flag, indicating whether authenticated requests are allowed by including the `Access-Control-Allow-Credentials` header. | "false"                 |
| HTSGET_DATA_SERVER_ALLOW_ORIGIN        | Which origin is allowed in the `ORIGIN` header.                                                                                 | "http://localhost:8081" |
| HTSGET_S3_BUCKET                       | The name of the AWS S3 bucket. Unused if HTSGET_STORAGE_TYPE is not "AwsS3Storage".                                             | ""                      |
| HTSGET_ID                              | ID of the service.                                                                                                              | "None"                  |
| HTSGET_NAME                            | Name of the service.                                                                                                            | "None"                  |
| HTSGET_VERSION                         | Version of the service.                                                                                                         | "None"                  |
| HTSGET_ORGANIZATION_NAME               | Name of the organization.                                                                                                       | "None"                  |
| HTSGET_ORGANIZATION_URL                | URL of the organization.                                                                                                        | "None"                  |
| HTSGET_CONTACT_URL                     | URL to provide contact to the users.                                                                                            | "None"                  |
| HTSGET_DOCUMENTATION_URL               | Link to documentation.                                                                                                          | "None"                  |
| HTSGET_CREATED_AT                      | Date of the creation of the service.                                                                                            | "None"                  |
| HTSGET_UPDATED_AT                      | Date of the last update of the service.                                                                                         | "None"                  |
| HTSGET_ENVIRONMENT                     | Environment in which the service is running.                                                                                    | "None"                  |

#### Example regular expression and substitution string

`HTSGET_REGEX` and `HTSGET_SUBSTITUTION_STRING` can be used to map between query IDs and 
the returned URL tickets.

For example, below is a `HTSGET_REGEX` variable which matches a `/` between two groups, and inserts an additional `data`
inbetween the groups with the `HTSGET_SUBSTITUTION_STRING`.

```sh
export HTSGET_REGEX='(?P<group1>.*?)/(?P<group2>.*)' 
export HTSGET_SUBSTITUTION_STRING='$group1/data/$group2'
```

For more information about regex options see the [regex crate](https://docs.rs/regex/).

#### RUST_LOG

The [Tracing][tracing] crate is used extensively by htsget-rs is for logging functionality. The `RUST_LOG` variable is
read to configure the level that trace logs are emitted.

For example, the following indicates trace level for all htsget crates, and info level for all other crates:

```sh
export RUST_LOG='info,htsget_http_lambda=trace,htsget_http_lambda=trace,htsget_config=trace,htsget_http_core=trace,htsget_search=trace,htsget_test_utils=trace'
```

See [here][rust-log] for more information on setting this variable.

[tracing]: https://github.com/tokio-rs/tracing
[rust-log]: https://rust-lang-nursery.github.io/rust-cookbook/development_tools/debugging/config_log.html

### As a library

This crate reads environment variables using [envy]. The main function for this is `from_env`, which can be used to 
obtain the `Config` struct. The crate also contains the `regex_resolver` abstraction, which is used for matching a query ID with
regex, and changing it by using a substitution string.

[envy]: https://github.com/softprops/envy

#### Feature flags

This crate has the following features:
* `s3-storage`: used to enable `AwsS3Storage` functionality.

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE