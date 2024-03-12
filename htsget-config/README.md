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

This crate is used to configure htsget-rs by using a config file or reading environment variables.

## Usage

### For running htsget-rs as an application

To configure htsget-rs, a TOML config file can be used. It also supports reading config from environment variables. 
Any config options set by environment variables override values in the config file. For some of
the more deeply nested config options, it may be more ergonomic to use a config file rather than environment variables.

The configuration consists of multiple parts, config for the ticket server, config for the data server, service-info config, and config for the resolvers.

#### Ticket server config
The ticket server responds to htsget requests by returning a set of URL tickets that the client must fetch and concatenate.
To configure the ticket server, set the following options:

| Option                                                                                        | Description                                                                                                                                                                                                | Type                                      | Default                     |
|-----------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------|-----------------------------|
| <span id="ticket_server_addr">`ticket_server_addr`</span>                                     | The address for the ticket server.                                                                                                                                                                         | Socket address                            | `'127.0.0.1:8080'`          | 
| <span id="ticket_server_tls">`ticket_server_tls`</span>                                       | Enable TLS for the ticket server. See [TLS](#tls) for more details.                                                                                                                                        | TOML table                                | Not enabled                 |
| <span id="ticket_server_cors_allow_credentials">`ticket_server_cors_allow_credentials`</span> | Controls the CORS Access-Control-Allow-Credentials for the ticket server.                                                                                                                                  | Boolean                                   | `false`                     |
| <span id="ticket_server_cors_allow_origins">`ticket_server_cors_allow_origins`</span>         | Set the CORS Access-Control-Allow-Origin returned by the ticket server, this can be set to `All` to send a wildcard, `Mirror` to echo back the request sent by the client, or a specific array of origins. | `'All'`, `'Mirror'` or a array of origins | `['http://localhost:8080']` |
| <span id="ticket_server_cors_allow_headers">`ticket_server_cors_allow_headers`</span>         | Set the CORS Access-Control-Allow-Headers returned by the ticket server, this can be set to `All` to allow all headers, or a specific array of headers.                                                    | `'All'`, or a array of headers            | `'All'`                     |
| <span id="ticket_server_cors_allow_methods">`ticket_server_cors_allow_methods`</span>         | Set the CORS Access-Control-Allow-Methods returned by the ticket server, this can be set to `All` to allow all methods, or a specific array of methods.                                                    | `'All'`, or a array of methods            | `'All'`                     |
| <span id="ticket_server_cors_max_age">`ticket_server_cors_max_age`</span>                     | Set the CORS Access-Control-Max-Age for the ticket server which controls how long a preflight request can be cached for.                                                                                   | Seconds                                   | `86400`                     |
| <span id="ticket_server_cors_expose_headers">`ticket_server_cors_expose_headers`</span>       | Set the CORS Access-Control-Expose-Headers returned by the ticket server, this can be set to `All` to expose all headers, or a specific array of headers.                                                  | `'All'`, or a array of headers            | `[]`                        |

TLS is supported by setting the `ticket_server_key` and `ticket_server_cert` options. An example of config for the ticket server:
```toml
ticket_server_addr = '127.0.0.1:8080'
ticket_server_cors_allow_credentials = false
ticket_server_cors_allow_origins = 'Mirror'
ticket_server_cors_allow_headers = ['Content-Type']
ticket_server_cors_allow_methods = ['GET', 'POST']
ticket_server_cors_max_age = 86400
ticket_server_cors_expose_headers = []
```

#### Local data server config
The local data server responds to tickets produced by the ticket server by serving local filesystem data. 
To configure the data server, set the following options:

| Option                                                                                    | Description                                                                                                                                                                                              | Type                                      | Default                     |
|-------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------|-----------------------------|
| <span id="data_server_addr">`data_server_addr`</span>                                     | The address for the data server.                                                                                                                                                                         | Socket address                            | `'127.0.0.1:8081'`          | 
| <span id="data_server_local_path">`data_server_local_path`</span>                         | The local path which the data server can access to serve files.                                                                                                                                          | Filesystem path                           | `'data'`                    |
| <span id="data_server_serve_at">`data_server_serve_at`</span>                             | The path which the data server will prefix to all response URLs for tickets.                                                                                                                             | URL path                                  | `'/data'`                   |
| <span id="data_server_tls">`data_server_tls`</span>                                       | Enable TLS for the data server. See [TLS](#tls) for more details.                                                                                                                                        | TOML table                                | Not enabled                 |
| <span id="data_server_cors_allow_credentials">`data_server_cors_allow_credentials`</span> | Controls the CORS Access-Control-Allow-Credentials for the data server.                                                                                                                                  | Boolean                                   | `false`                     |
| <span id="data_server_cors_allow_origins">`data_server_cors_allow_origins`</span>         | Set the CORS Access-Control-Allow-Origin returned by the data server, this can be set to `All` to send a wildcard, `Mirror` to echo back the request sent by the client, or a specific array of origins. | `'All'`, `'Mirror'` or a array of origins | `['http://localhost:8080']` |
| <span id="data_server_cors_allow_headers">`data_server_cors_allow_headers`</span>         | Set the CORS Access-Control-Allow-Headers returned by the data server, this can be set to `All` to allow all headers, or a specific array of headers.                                                    | `'All'`, or a array of headers            | `'All'`                     |
| <span id="data_server_cors_allow_methods">`data_server_cors_allow_methods`</span>         | Set the CORS Access-Control-Allow-Methods returned by the data server, this can be set to `All` to allow all methods, or a specific array of methods.                                                    | `'All'`, or a array of methods            | `'All'`                     |
| <span id="data_server_cors_max_age">`data_server_cors_max_age`</span>                     | Set the CORS Access-Control-Max-Age for the data server which controls how long a preflight request can be cached for.                                                                                   | Seconds                                   | `86400`                     |
| <span id="data_server_cors_expose_headers">`data_server_cors_expose_headers`</span>       | Set the CORS Access-Control-Expose-Headers returned by the data server, this can be set to `All` to expose all headers, or a specific array of headers.                                                  | `'All'`, or a array of headers            | `[]`                        |

TLS is supported by setting the `data_server_key` and `data_server_cert` options.  An example of config for the data server:
```toml
data_server_addr = '127.0.0.1:8081'
data_server_local_path = 'data'
data_server_serve_at = '/data'
data_server_key = 'key.pem'
data_server_cert = 'cert.pem'
data_server_cors_allow_credentials = false
data_server_cors_allow_origins = 'Mirror'
data_server_cors_allow_headers = ['Content-Type']
data_server_cors_allow_methods = ['GET', 'POST']
data_server_cors_max_age = 86400
data_server_cors_expose_headers = []
```

Sometimes it may be useful to disable the data server as all responses to the ticket server will be handled elsewhere, such as with an AWS S3 data server.

To disable the data server, set the following option:

<pre id="data_server" lang="toml">
data_server_enabled = false
</pre>

#### Service info config

The service info config controls what is returned when the [`service-info`][service-info] path is queried.<br>
To configure the service-info, set the following options:

| Option                                                  | Description                                 | Type      | Default  |
|---------------------------------------------------------|---------------------------------------------|-----------|----------|
| <span id="id">`id`</span>                               | Service ID.                                 | String    | Not set  | 
| <span id="name">`name`</span>                           | Service name.                               | String    | Not set  |
| <span id="version">`version`</span>                     | Service version.                            | String    | Not set  |
| <span id="organization_name">`organization_name`</span> | Organization name.                          | String    | Not set  |
| <span id="organization_url">`organization_url`</span>   | Organization URL.                           | String    | Not set  |
| <span id="contact_url">`contact_url`</span>             | Service contact URL                         | String    | Not set  |
| <span id="documentation_url">`documentation_url`</span> | Service documentation URL.                  | String    | Not set  |
| <span id="created_at">`created_at`</span>               | When the service was created.               | String    | Not set  |
| <span id="updated_at">`updated_at`</span>               | When the service was last updated.          | String    | Not set  |
| <span id="environment">`environment`</span>             | The environment the service is running in.  | String    | Not set  |

An example of config for the service info:
```toml
id = 'id'
name = 'name'
version = '0.1'
organization_name = 'name'
organization_url = 'https://example.com/'
contact_url = 'mailto:nobody@example.com'
documentation_url = 'https://example.com/'
created_at = '2022-01-01T12:00:00Z'
updated_at = '2022-01-01T12:00:00Z'
environment = 'dev'
```

#### Resolvers

The resolvers component of htsget-rs is used to map query IDs to the location of the resource. Each query that htsget-rs receives is
'resolved' to a location, which a data server can respond with. A query ID is matched with a regex, and is then mapped with a substitution string that
has access to the regex capture groups. Resolvers are configured in an array, where the first matching resolver is resolver used to map the ID.

To create a resolver, add a `[[resolvers]]` array of tables, and set the following options:

| Option                | Description                                                                                                             | Type                                  | Default |
|-----------------------|-------------------------------------------------------------------------------------------------------------------------|---------------------------------------|---------|
| `regex`               | A regular expression which can match a query ID.                                                                        | Regex                                 | '.*'    | 
| `substitution_string` | The replacement expression used to map the matched query ID. This has access to the match groups in the `regex` option. | String with access to capture groups  | '$0'    |

For example, below is a `regex` option which matches a `/` between two groups, and inserts an additional `data`
inbetween the groups with the `substitution_string`.

```toml
[[resolvers]]
regex = '(?P<group1>.*?)/(?P<group2>.*)'
substitution_string = '$group1/data/$group2'
```

For more information about regex options see the [regex crate](https://docs.rs/regex/).

Each resolver also maps to a certain storage backend. This storage backend can be used to set query IDs which are served from local storage, from S3-style bucket storage, or from HTTP URLs.
To set the storage backend for a resolver, add a `[resolvers.storage]` table. Some storage backends require feature flags to be set when compiling htsget-rs.

To use `LocalStorage`, set `storage = 'Local'`. This will derive the values for the fields below from the `data_server` config:

| Option              | Description                                                                                                                         | When `storage = 'Local'`                                                                                                         | Type                         | Default            |
|---------------------|-------------------------------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------|------------------------------|--------------------|
| `scheme`            | The scheme present on URL tickets.                                                                                                  | Derived from `data_server_key` and `data_server_cert`. If no key and cert are present, then uses `Http`, otherwise uses `Https`. | Either `'Http'` or `'Https'` | `'Http'`           |
| `authority`         | The authority present on URL tickets. This should likely match the `data_server_addr`.                                              | Same as `data_server_addr`.                                                                                                      | URL authority                | `'127.0.0.1:8081'` |
| `local_path`        | The local filesystem path which the data server uses to respond to tickets.  This should likely match the `data_server_local_path`. | Same as `data_server_local_path`.                                                                                                | Filesystem path              | `'data'`           |
| `path_prefix`       | The path prefix which the URL tickets will have. This should likely match the `data_server_serve_at` path.                          | Same as `data_server_serve_at`.                                                                                                  | URL path                     | `'/data'`          |

To use `S3Storage`, build htsget-rs with the `s3-storage` feature enabled, and set `storage = 'S3'`. This will derive the value for `bucket` from the `regex` component of the `resolvers`:

| Option       | Description                                                                                                                                                                   | When `storage = 'S3'`                                                                                            | Type    | Default                                |
|--------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------|---------|----------------------------------------|
| `bucket`     | The AWS S3 bucket where resources can be retrieved from.                                                                                                                      | Derived from the `resolvers` `regex` property. This uses the first capture group in the `regex` as the `bucket`. | String  | `''`                                   |
| `endpoint`   | A custom endpoint to override the default S3 service address. This is useful for using S3 locally or with storage backends such as MinIO. See [MinIO](#minio).                | Not set, uses regular AWS S3 services.                                                                           | String  | Not set, uses regular AWS S3 services. |
| `path_style` | The S3 path style to request from the storage backend. If `true`, "path style" is used, e.g. `host.com/bucket/object.bam`, otherwise `bucket.host.com/object` style is used.  | `false`                                                                                                          | Boolean | `false`                                |

`UrlStorage` is another storage backend which can be used to serve data from a remote HTTP URL. When using this storage backend, htsget-rs will fetch data from a `url` which is set in the config. It will also forward any headers received with the initial query, which is useful for authentication. 
To use `UrlStorage`, build htsget-rs with the `url-storage` feature enabled, and set the following options under `[resolvers.storage]`:

| Option                                            | Description                                                                                                                                                      | Type       | Default                                                                                                   |
|---------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------|-----------------------------------------------------------------------------------------------------------|
| <span id="endpoint_index">`endpoint_index`</span> | The URL to fetch index for a file. The request will be a GET request which expects the index file specific to a BAM/CRAM/VCF file.                               | HTTP URL   | `"https://127.0.0.1:8081/"`                                                                               |
| <span id="endpoint_header">`endpoint_file`</span> | The URL to fetch underlying for a file. The request will be a GET request which expects to get the decrypted underlying header from a BAM/CRAM/VCF file.         | HTTP URL   | `"https://127.0.0.1:8081/"`                                                                               |
| <span id="url">`response_url`</span>              | The URL to return to the client for fetching tickets.                                                                                                            | HTTP URL   | `"https://127.0.0.1:8081/"`                                                                               |
| `forward_headers`                                 | When constructing the URL tickets, copy HTTP headers received in the initial query. Note, the headers received with the query are always forwarded to the `url`. | Boolean    | `true`                                                                                                    |
| `user_agent`                                      | A user agent to provide when making requests to the URLs.                                                                                                        | String     | A combination of the cargo package name and version. For example, `htsget-search/0.6.6`.                       |
| `tls`                                             | Additionally enables client authentication, or sets non-native root certificates for TLS. See [TLS](#tls) for more details.                                      | TOML table | TLS is always allowed, however the default performs no client authentication and uses native root certificates. |

When using `UrlStorage`, the following requests will be made:
* `GET` request to fetch only the crypt4gh headers size of the data file (e.g. `GET /data.bam`), URL used is configured via `endpoint_crypt4gh_header`.
* `GET` request to fetch only the headers of the data file (e.g. `GET /data.bam`, with `Range: bytes=0-<end_of_bam_header>`), URL used is configured via `endpoint_header`.
* `GET` request to fetch the entire index file (e.g. `GET /data.bam.bai`), URL used is configured via `endpoint_index`.
* `HEAD` request on the data file to get its length (e.g. `HEAD /data.bam`), URL used is configured via `endpoint_head`.

All headers received in the initial query will be included when making these requests.

For example, a `resolvers` value of:
```toml
[[resolvers]]
regex = '^(example_bucket)/(?P<key>.*)$'
substitution_string = '$key'
storage = 'S3'
```
Will use "example_bucket" as the S3 bucket if that resolver matches, because this is the first capture group in the `regex`.
Note, to use this feature, at least one capture group must be defined in the `regex`.

Note, all the values for `S3Storage` or `LocalStorage` can be also be set manually by adding a
`[resolvers.storage]` table. For example, to manually set the config for `LocalStorage`:

```toml
[[resolvers]]
regex = '.*'
substitution_string = '$0'

[resolvers.storage]
scheme = 'Http'
authority = '127.0.0.1:8081'
local_path = 'data'
path_prefix = '/data'
```

or, to manually set the config for `S3Storage`:

```toml
[[resolvers]]
regex = '.*'
substitution_string = '$0'

[resolvers.storage]
bucket = 'bucket'
```

`UrlStorage` can only be specified manually.

There are additional examples of config files located under [`examples/config-files`][examples-config-files].

#### Note
By default, when htsget-rs is compiled with the `s3-storage` feature flag, `storage = 'S3'` is used when no `storage` options
are specified. Otherwise, `storage = 'Local'` is used when no storage options are specified. Compilation includes the `s3-storage` 
feature flag by default, so in order to have `storage = 'Local'` as the default, `--no-default-features` can be passed to `cargo`.

#### Allow guard
Additionally, the resolver component has a feature, which allows resolving IDs based on the other fields present in a query.
This is useful as allows the resolver to match an ID, if a particular set of query parameters are also present. For example, 
a resolver can be set to only resolve IDs if the format is also BAM.

This component can be configured by setting the `[resolver.allow_guard]` table. The following options are available to restrict which queries are resolved by a resolver:

| Option                  | Description                                                                             | Type                                                                  | Default                             |
|-------------------------|-----------------------------------------------------------------------------------------|-----------------------------------------------------------------------|-------------------------------------|
| `allow_reference_names` | Resolve the query ID if the query also contains the reference names set by this option. | Array of reference names or `'All'`                                   | `'All'`                             | 
| `allow_fields`          | Resolve the query ID if the query also contains the fields set by this option.          | Array of fields or `'All'`                                            | `'All'`                             |
| `allow_tags`            | Resolve the query ID if the query also contains the tags set by this option.            | Array of tags or `'All'`                                              | `'All'`                             |
| `allow_formats`         | Resolve the query ID if the query is one of the formats specified by this option.       | An array of formats containing `'BAM'`, `'CRAM'`, `'VCF'`, or `'BCF'` | `['BAM', 'CRAM', 'VCF', 'BCF']`     |
| `allow_classes`         | Resolve the query ID if the query is one of the classes specified by this option.       | An array of classes containing eithr `'body'` or `'header'`           | `['body', 'header']`                |
| `allow_interval_start`  | Resolve the query ID if the query reference start position is at least this option.     | Unsigned 32-bit integer start position, 0-based, inclusive            | Not set, allows all start positions |
| `allow_interval_end`    | Resolve the query ID if the query reference end position is at most this option.        | Unsigned 32-bit integer end position, 0-based exclusive.              | Not set, allows all end positions   |

An example of a fully configured resolver:

```toml
[[resolvers]]
regex = '.*'
substitution_string = '$0'

[resolvers.storage]
bucket = 'bucket'

[resolvers.allow_guard]
allow_reference_names = ['chr1']
allow_fields = ['QNAME']
allow_tags = ['RG']
allow_formats = ['BAM']
allow_classes = ['body']
allow_interval_start = 100
allow_interval_end = 1000
```

In this example, the resolver will only match the query ID if the query is for `chr1` with positions between `100` and `1000`.

#### TLS

TLS can be configured for the ticket server, data server, or the url storage client. These options read private keys and
certificates from PEM-formatted files. Certificates must be in X.509 format and private keys can be RSA, PKCS8, or SEC1 (EC) encoded. 
The following options are available:

| Option                 | Description                                                                                                                               | Type              | Default |
|------------------------|-------------------------------------------------------------------------------------------------------------------------------------------|-------------------|---------|
| `key`                  | The path to the PEM formatted X.509 certificate. Specifies TLS for servers or client authentication for clients.                          | Filesystem path   | Not Set | 
| `cert`                 | The path to the PEM formatted RSA, PKCS8, or SEC1 encoded EC private key. Specifies TLS for servers or client authentication for clients. | Filesystem path   | Not Set |
| `root_store`           | The path to the PEM formatted root certificate store. Only used to specify non-native root certificates for client TLS.                   | Filesystem path   | Not Set |

When used by the ticket and data servers, `key` and `cert` enable TLS, and when used with the url storage client, they enable client authentication.
The root store is only used by the url storage client. Note, the url storage client always allows TLS, however the default configuration performs no client authentication
and uses the native root certificate store.

For example, TLS for the ticket server can be enabled by specifying the key and cert options:
```toml
ticket_server_tls.cert = "cert.pem"
ticket_server_tls.key = "key.pem"
```

Further TLS examples are available under [`examples/config-files`][examples-config-files].

[examples-config-files]: examples/config-files

#### Object type
There is additional configuration that changes the way a resolver treats an object.

By default, all objects are considered `Regular`. However, the `object_type` can be configured to decrypt Crypt4GH files.

This component can be configured by setting the `[resolver.object_type]` table in order to enable Crypt4GH:

| Option                     | Description                                                                                                                                                                               | Type    | Default                                        |
|----------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|---------|------------------------------------------------|
| `send_encrypted_to_client` | Whether to send data encrypted byte ranges to the client. Note, this does not affect data sent to the `UrlStorage` backend, which remains encrypted if type Crypt4GH object type is used. | Boolean | Not set                                        |
| `private_key`              | Path to the private key used for decrypted Crypt4GH data.                                                                                                                                 | Path    | Not set, generates ephemeral keys if not set.  | 
| `public_key`               | Path to the public key used for decrypted Crypt4GH data.                                                                                                                                  | Path    | Not set, generates ephemeral keys if not set.  | 

Or, to generate keys uniquely for each request, the `private_key` and `public_key` options should not be set.

For example to enable Crypt4GH for a resolver, build htsget-rs with the `crypt4gh` feature enabled, and set the following options under `[resolvers.object_type]`:

```toml
[resolvers.object_type]
# Specify the keys that htsget will use manually.
send_encrypted_to_client = true
private_key = "data/crypt4gh/keys/bob.sec" # pragma: allowlist secret
public_key = "data/crypt4gh/keys/bob.pub"
```

Note, currently this functionality only works with `UrlStorage`.

#### Config file location

The htsget-rs binaries ([htsget-actix] and [htsget-lambda]) support some command line options. The config file location can
be specified by setting the `--config` option:

```shell
cargo run -p htsget-actix -- --config "config.toml"
```

The config can also be read from an environment variable:

```shell
export HTSGET_CONFIG="config.toml"
```
If no config file is specified, the default configuration is used. Further, the default configuration file can be printed to stdout by passing
the `--print-default-config` flag:

```shell
cargo run -p htsget-actix -- --print-default-config
```

Use the `--help` flag to see more details on command line options.

[htsget-actix]: ../htsget-actix
[htsget-lambda]: ../htsget-lambda

#### Log formatting

The [Tracing][tracing] crate is used extensively by htsget-rs is for logging functionality. The `RUST_LOG` variable is
read to configure the level that trace logs are emitted.

For example, the following indicates trace level for all htsget crates, and info level for all other crates:

```sh
export RUST_LOG='info,htsget_lambda=trace,htsget_lambda=trace,htsget_config=trace,htsget_http=trace,htsget_search=trace,htsget_test=trace'
```

See [here][rust-log] for more information on setting this variable.

The style of formatting can be configured by setting the following option:

| Option                                                  | Description                          | Type                                                   | Default  |
|---------------------------------------------------------|--------------------------------------|--------------------------------------------------------|----------|
| <span id="formatting_style">`formatting_style`</span>   | The style of log formatting to use.  | One of `'Full'`, `'Compact'`, `'Pretty'`, or `'Json'`  | `'Full'` |

See [here][formatting-style] for more information on how these values look.

[tracing]: https://github.com/tokio-rs/tracing
[rust-log]: https://rust-lang-nursery.github.io/rust-cookbook/development_tools/debugging/config_log.html
[formatting-style]: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/fmt/index.html#formatters

#### Configuring htsget-rs with environment variables

All the htsget-rs config options can be set by environment variables, which is convenient for runtimes such as AWS Lambda.
The ticket server, data server and service info options are flattened and can be set directly using 
environment variable. It is not recommended to set the resolvers using environment variables, however it can be done by setting a single environment variable which 
contains a list of structures, where a key name and value pair is used to set the nested options.

Environment variables will override options set in the config file. Note, arrays are delimited with `[` and `]` in environment variables, and items are separated by commas.

The following environment variables - corresponding to the TOML config - are available:

| Variable                                      | Description                                                                         |
|-----------------------------------------------|-------------------------------------------------------------------------------------|
| `HTSGET_TICKET_SERVER_ADDR`                   | See [`ticket_server_addr`](#ticket_server_addr)                                     | 
| `HTSGET_TICKET_SERVER_TLS_KEY`                | See [`TLS`](#tls)                                                                   |
| `HTSGET_TICKET_SERVER_TLS_CERT`               | See [`TLS`](#tls)                                                                   |
| `HTSGET_TICKET_SERVER_CORS_ALLOW_CREDENTIALS` | See [`ticket_server_cors_allow_credentials`](#ticket_server_cors_allow_credentials) |
| `HTSGET_TICKET_SERVER_CORS_ALLOW_ORIGINS`     | See [`ticket_server_cors_allow_origins`](#ticket_server_cors_allow_origins)         |
| `HTSGET_TICKET_SERVER_CORS_ALLOW_HEADERS`     | See [`ticket_server_cors_allow_headers`](#ticket_server_cors_allow_headers)         |
| `HTSGET_TICKET_SERVER_CORS_ALLOW_METHODS`     | See [`ticket_server_cors_allow_methods`](#ticket_server_cors_allow_methods)         |
| `HTSGET_TICKET_SERVER_CORS_MAX_AGE`           | See [`ticket_server_cors_max_age`](#ticket_server_cors_max_age)                     |
| `HTSGET_TICKET_SERVER_CORS_EXPOSE_HEADERS`    | See [`ticket_server_cors_expose_headers`](#ticket_server_cors_expose_headers)       |
| `HTSGET_DATA_SERVER_ADDR`                     | See [`data_server_addr`](#data_server_addr)                                         |
| `HTSGET_DATA_SERVER_LOCAL_PATH`               | See [`data_server_local_path`](#data_server_local_path)                             |
| `HTSGET_DATA_SERVER_SERVE_AT`                 | See [`data_server_serve_at`](#data_server_serve_at)                                 |
| `HTSGET_DATA_SERVER_TLS_KEY`                  | See [`TLS`](#tls)                                                                   |
| `HTSGET_DATA_SERVER_TLS_CERT`                 | See [`TLS`](#tls)                                                                   |
| `HTSGET_DATA_SERVER_CORS_ALLOW_CREDENTIALS`   | See [`data_server_cors_allow_credentials`](#data_server_cors_allow_credentials)     |
| `HTSGET_DATA_SERVER_CORS_ALLOW_ORIGINS`       | See [`data_server_cors_allow_origins`](#data_server_cors_allow_origins)             |
| `HTSGET_DATA_SERVER_CORS_ALLOW_HEADERS`       | See [`data_server_cors_allow_headers`](#data_server_cors_allow_headers)             |
| `HTSGET_DATA_SERVER_CORS_ALLOW_METHODS`       | See [`data_server_cors_allow_methods`](#data_server_cors_allow_methods)             |
| `HTSGET_DATA_SERVER_CORS_MAX_AGE`             | See [`data_server_cors_max_age`](#data_server_cors_max_age)                         |
| `HTSGET_DATA_SERVER_CORS_EXPOSE_HEADERS`      | See [`data_server_cors_expose_headers`](#data_server_cors_expose_headers)           |
| `HTSGET_ID`                                   | See [`id`](#id)                                                                     |
| `HTSGET_NAME`                                 | See [`name`](#name)                                                                 |
| `HTSGET_VERSION`                              | See [`version`](#version)                                                           |
| `HTSGET_ORGANIZATION_NAME`                    | See [`organization_name`](#organization_name)                                       |
| `HTSGET_ORGANIZATION_URL`                     | See [`organization_url`](#organization_url)                                         |
| `HTSGET_CONTACT_URL`                          | See [`contact_url`](#contact_url)                                                   |
| `HTSGET_DOCUMENTATION_URL`                    | See [`documentation_url`](#documentation_url)                                       |
| `HTSGET_CREATED_AT`                           | See [`created_at`](#created_at)                                                     |
| `HTSGET_UPDATED_AT`                           | See [`updated_at`](#updated_at)                                                     |
| `HTSGET_ENVIRONMENT`                          | See [`environment`](#environment)                                                   |
| `HTSGET_RESOLVERS`                            | See [`resolvers`](#resolvers)                                                       |
| `HTSGET_FORMATTING_STYLE`                     | See [`formatting_style`](#formatting_style)                                         |

In order to use `HTSGET_RESOLVERS`, the entire resolver config array must be set. The nested array of resolvers structure can be set using name key and value pairs, for example:

```shell
export HTSGET_RESOLVERS="[{
    regex=regex,
    substitution_string=substitution_string,
    storage={
        bucket=bucket
    },
    allow_guard={
        allow_reference_names=[chr1],
        allow_fields=[QNAME],
        allow_tags=[RG],
        allow_formats=[BAM],
        allow_classes=[body],
        allow_interval_start=100,
        allow_interval_end=1000
    }  
}]"
```

Similar to the [data_server](#data_server) option, the data server can be disabled by setting the equivalent environment variable:

```shell
export HTSGET_DATA_SERVER_ENABLED=false
```
[service-info]: https://samtools.github.io/hts-specs/htsget.html#ga4gh-service-info

### MinIO

Operating a local object storage like [MinIO][minio] can be easily achieved by leveraging the `endpoint` directive as shown below:

```toml
[[resolvers]]
regex = ".*"
substitution_string = "$0"

[resolvers.storage]
bucket = 'bucket'
endpoint = "http://127.0.0.1:9000"
```

This will have htsget-rs behaving like the native AWS CLI, i.e:

```
mkdir /tmp/test
minio server /tmp/test
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
aws s3 mb --endpoint-url=http://localhost:9000 s3://bucket/
aws s3 cp --recursive --endpoint-url=http://localhost:9000 htsget-rs/data/bam s3://bucket/
cargo run -p htsget-actix -- --config ~/.htsget-rs/config.toml

# On another session/terminal
curl http://localhost:8080/reads/htsnexus_test_NA12878
```

Please don't run the example above as-is in production systems ;)

### As a library

This crate reads config files and environment variables using [figment], and accepts command-line arguments using clap. The main function for this is `from_config`,
which is used to obtain the `Config` struct. The crate also contains the `regex_resolver` abstraction, which is used for matching a query ID with
regex, and changing it by using a substitution string.

[figment]: https://github.com/SergioBenitez/Figment

#### Feature flags

This crate has the following features:
* `s3-storage`: used to enable `S3Storage` functionality.
* `url-storage`: used to enable `UrlStorage` functionality.
* `crypt4gh`: used to enable Crypt4GH functionality.

## License

This project is licensed under the [MIT license][license].

[license]: LICENSE
[minio]: https://min.io/
