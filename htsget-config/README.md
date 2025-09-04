# htsget-config

[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/umccr/htsget-rs/blob/main/LICENSE
[actions-badge]: https://github.com/umccr/htsget-rs/actions/workflows/action.yml/badge.svg
[actions-url]: https://github.com/umccr/htsget-rs/actions?query=workflow%3Atests+branch%3Amain

## Overview

Configuration for [htsget-rs].

[htsget-rs]: https://github.com/umccr/htsget-rs

## Quickstart
The simplest way to use htsget-rs is to create a [toml] config file and specify a storage location:

```toml
locations = "file://data"
```

Then launch the server using the config file:

```sh
cargo run --all-features -p htsget-axum -- --config <your_config_file.toml>
```

This will serve files under the [`data`][data] directory:

```sh
curl 'http://localhost:8080/reads/bam/seraseq_cebpa_larger'
```

Locations allow htsget-rs access to bioinformatics files and indexes. Instead of local files, htsget-rs can access
files on s3, which returns pre-signed URLs for tickets:

```toml
locations = "s3://bucket"
```

or on a remote HTTP server (either `http://` or `https://`):

```toml
locations = "https://example.com"
```

Multiple locations can be specified by providing a list and an id prefix after the location:

```toml
locations = [ "file://data/bam", "file://data/cram" ]
```

This allows htsget-rs to serve data only when the request also contains the prefix:

```sh
curl 'http://localhost:8080/reads/bam/seraseq_cebpa_larger'
curl 'http://localhost:8080/reads/cram/seraseq_cebpa_larger?format=CRAM'
```

Locations can be mixed, and don't all need to have the same directory or resource:

```toml
locations = [ "file://data/bam", "file://data/cram", "s3://bucket/vcf" ]
```

For all types of locations, the second path segment represents the prefix which the request is expected
to contain, in order to use that location. With the above example, if the request is `/variants/vcf/<id>`, then
the S3 location is used, and if it is `/reads/bam/<id>` or `/reads/cram/<id>`, then the file locations are used.

For each of the location types, the first component represents the storage location:

```toml
locations = [ "file://<directory>/<prefix>", "s3://<bucket>/<prefix>", "https://<endpoint>/<prefix>" ]
```

htsget-rs spawns a separate server process to respond to htsget tickets for file locations.
This server's path can be set by using `data_server.local_path`. When using `file://<directory>` locations,
the directory component must be the same as the local path so that the server has access to it. 
It is also not possible to have different directories components when using multiple `file://<directory>`
locations.

The data server process can be disabled by setting it to `None` if no file locations are being used:

```toml
data_server = "None"
```

This is automatically applied if no file locations are configured.

By default, file locations specified via `file://<dir>` will use the data server scheme and
address for ticket responses. This means that tickets will be served as `<scheme>://<addr>/reads/<id>`,
pointing to the data server `<scheme>` and `<addr>` automatically. For example, a default `file://data`
location will have tickets that look like `http://127.0.0.1:8081/reads/<id>`.

The scheme and address can be overridden for any file-based responses by setting `data_server.ticket_origin`:

```toml
# Ensure that the scheme is set for the origin.
data_server.ticket_origin = "https://example.com/"
## The url can also have a path set, which appears in the tickets.
#data_server.ticket_origin = "https://example.com/path"
```

In this example, the tickets will appear as `https://example.com/<id>`. This is useful to arbitrarily route tickets to
DNS-resolvable requests, for example, inside a docker container.

> [!NOTE]  
> For S3 locations, the bucket is not included in the request to htsget-rs. To include the bucket as well, 
> see deriving the bucket from the first capture group in [advanced config](#bucket).
 
> [!IMPORTANT]  
> Some parts of htsget-rs require extra feature flags for conditional compilation, that's why the examples specify
> using `--all-features`. Notably, `--features aws` enables the `S3` location type, and `--features url`
> enabled the remote HTTP server location type. If using a subset of features, for example S3 locations only, then
> a single feature can be enabled instead of using `--all-features`.

### Server config

htsget-rs spawn up to two server instances - the ticket server responds to the initial htsget request, and optionally,
the data server, which responds to the htsget tickets.

The socket address of the servers can be changed by specifying `addr`:

```toml
ticket_server.addr = "127.0.0.1:8000"
data_server.addr = "127.0.0.1:8001"
```

TLS can be configured to enabled HTTPS support by providing a certificate and private key:

```toml
ticket_server.tls.key = "key.pem"
ticket_server.tls.cert = "cert.pem"

data_server.tls.key = "key.pem"
data_server.tls.cert = "cert.pem"
```

### Service info config

The service info config controls what is returned when the [`service-info`][service-info] path is queried. The following
option accepts [GA4GH service info][service-info] values or any [custom nested value][service-info-custom], which get
converted to a JSON response:

```toml
service_info.id = "org.ga4gh.htsget"
service_info.environment = "dev"
service_info.organization = { name = "name", url = "https://example.com/" }
service_info.custom = { data = "data", number = "123" }
```

The `service_info` option does not have to be specified. Any required fields that are part of the [service info][service-info]
spec and some optional ones are pre-filled from the Rust package info. For example, the `version` field is set to the current
crate version and `id` is set to `<package_name>/<package_version`. It is recommended to set the `service_info.id` field
to a custom value as the package name and version are not globally unique.

[service-info]: https://github.com/ga4gh-discovery/ga4gh-service-info
[service-info-custom]: https://github.com/ga4gh-discovery/ga4gh-service-info/blob/develop/service-info.yaml

### Environment variables

Most options can also be set using environment variables. Any environment variables will override options set in the
config file. Arrays are delimited with `[` and `]`, and items are separated by commas:

| Variable                        | Description                                                    | Example                                            |
|---------------------------------|----------------------------------------------------------------|----------------------------------------------------|
| `HTSGET_TICKET_SERVER_ADDR`     | Set the ticket server socket address.                          | "127.0.0.1:8080"                                   |
| `HTSGET_TICKET_SERVER_TLS_KEY`  | See [server config](#server-config)                            | "key.pem"                                          |
| `HTSGET_TICKET_SERVER_TLS_CERT` | See [server config](#server-config)                            | "cert.pem"                                         |
| `HTSGET_DATA_SERVER_ADDR`       | Set the data server socket address.                            | "127.0.0.1:8081"                                   |
| `HTSGET_DATA_SERVER_LOCAL_PATH` | Set the path that the data server has access to.               | "dir/path"                                         |
| `HTSGET_DATA_SERVER_TLS_KEY`    | See [server config](#server-config)                            | "key.pem"                                          |
| `HTSGET_DATA_SERVER_TLS_CERT`   | See `server config](#server-config)                            | "cert.pem"                                         |
| `HTSGET_SERVICE_INFO`           | Set the service info, see [service info](#service-info-config) | "{ organization = { name = name, url = url }}"     |
| `HTSGET_LOCATIONS`              | Set the locations.                                             | "[file://data/prefix_one, s3://bucket/prefix_two]" |
| `HTSGET_CONFIG`                 | Set the config file location.                                  | "dir/config.toml"                                  |

## Advanced config

The following section describes advanced configuration which is more flexible, but adds complexity.

### Regex-based location

Instead of the simple path-based locations described above, htsget-rs supports arbitrary regex-based id resolution.
This allows matching an [`id`][id], which is everything after `reads/` or `variants/` in the http path, and mapping
it to a location using regex substitution.

To create a regex location, add a `[[locations]]` array of tables, and set the following options:

| Option                | Description                                                                                                             | Type                                  | Default |
|-----------------------|-------------------------------------------------------------------------------------------------------------------------|---------------------------------------|---------|
| `regex`               | A regular expression which can match a query ID.                                                                        | Regex                                 | `'.*'`  | 
| `substitution_string` | The replacement expression used to map the matched query ID. This has access to the match groups in the `regex` option. | String with access to capture groups  | `'$0'`  |

For example, below is a `regex` option which matches a `/` between two groups, and inserts an additional `data`
in between the groups with the `substitution_string`:

```toml
[[locations]]
regex = '(?P<group1>.*?)/(?P<group2>.*)'
substitution_string = '$group1/data/$group2'
```

This would mean that a request to `http://localhost:8080/reads/some_id/file` would search for files at `some_id/data/file.bam`.

The regex locations also have access to further configuration of storage locations for `file://`, `s3://`, or `http://`
locations. These are called `File`, `S3`, and `Url` respectively.

To manually configure `File` locations, set `backend.kind = "File"`, and specify any additional options from below the `backend` table:

| Option                   | Description                                                                                                               | Type                         | Default            |
|--------------------------|---------------------------------------------------------------------------------------------------------------------------|------------------------------|--------------------|
| `scheme`                 | The scheme present on URL tickets.                                                                                        | Either `'Http'` or `'Https'` | `'Http'`           |
| `authority`              | The authority present on URL tickets. This should likely match the `data_server.addr`.                                    | URL authority                | `'127.0.0.1:8081'` |
| `local_path`             | The local filesystem path which the data server uses to respond to tickets. This must match the `data_server.local_path`. | Filesystem path              | `'./'`             |

For example:

```toml
data_server.addr = "127.0.0.1:8000"

[[locations]]
regex = ".*"
substitution_string = "$0"

backend.kind = "File"
backend.scheme = "Http"
backend.authority = "127.0.0.1:8000"
backend.local_path = "path"
```

To manually configure `S3` locations, set `backend.kind = "S3"`, and specify options from below under the `backend` table:

| Option                             | Description                                                                                                                                                                   | Type    | Default                                                                                                                  |
|------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|---------|--------------------------------------------------------------------------------------------------------------------------|
| <span id="bucket">`bucket`</span>  | The AWS S3 bucket where resources can be retrieved from.                                                                                                                      | String  | Derived from the `location` `regex` property if empty. This uses the first capture group in the `regex` as the `bucket`. |
| `endpoint`                         | A custom endpoint to override the default S3 service address. This is useful for using S3 locally or with storage backends such as MinIO. See [MinIO](#minio).                | String  | Not set, uses regular AWS S3 services.                                                                                   |
| `path_style`                       | The S3 path style to request from the storage backend. If `true`, "path style" is used, e.g. `host.com/bucket/object.bam`, otherwise `bucket.host.com/object` style is used.  | Boolean | `false`                                                                                                                  |

For example, the following backend manually sets the `bucket` and uses path style requests:

```toml
[[locations]]
regex = "prefix/(?P<key>.*)$"
substitution_string = "$key"

backend.kind = "S3"
backend.bucket = "bucket"
backend.path_style = true
```

To manually configure `Url` locations, set `backend.kind = "Url"`, specify any additional options from below under the `backend` table:

| Option                               | Description                                                                                                                                                   | Type                     | Default                                                                                                         |
|--------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------|-----------------------------------------------------------------------------------------------------------------|
| <span id="url">`url`</span>          | The URL to fetch data from.                                                                                                                                   | HTTP URL                 | `"https://127.0.0.1:8081/"`                                                                                     |
| <span id="url">`response_url`</span> | The URL to return to the client for fetching tickets.                                                                                                         | HTTP URL                 | `"https://127.0.0.1:8081/"`                                                                                     |
| `forward_headers`                    | When constructing the URL tickets, copy HTTP headers received in the initial query.                                                                           | Boolean                  | `true`                                                                                                          |
| `header_blacklist`                   | List of headers that should not be forwarded.                                                                                                                 | Array of headers         | `[]`                                                                                                            |
| `tls`                                | Additionally enables client authentication, or sets non-native root certificates for TLS. See [server configuration](#server-configuration) for more details. | TOML table               | TLS is always allowed, however the default performs no client authentication and uses native root certificates. |

For example, the following forwards all headers to response tickets except `Host`, and constructs tickets using `https://example.com` instead of `http://localhost:8080`:

```toml
[[locations]]
regex = ".*"
substitution_string = "$0"

backend.kind = "Url"
backend.url = "http://localhost:8080"
backend.response_url = "https://example.com"
backend.forward_headers = true
backend.header_blacklist = ["Host"]
```

Regex-based locations also support multiple locations:

```toml
[[locations]]
regex = "prefix/(?P<key>.*)$"
substitution_string = "$key"
backend.kind = "S3"
backend.bucket = "bucket"
backend.path_style = true

[[locations]]
regex = ".*"
substitution_string = "$0"
backend.kind = "Url"
backend.url = "http://localhost:8080"
forward_headers = false
```

If there is an overlap in regex matches, the first location specified will be the one used.

Additional config file examples are available under [`example/config-files`][examples-config-files].

### Allow guard

Additionally, locations support resolving IDs based on the other fields present in a query.
This is useful to allow the location to match an ID only if a particular set of query parameters are also present.

This component can be configured by setting the `guard` table with:

| Option                  | Description                                                                             | Type                                                                  | Default                             |
|-------------------------|-----------------------------------------------------------------------------------------|-----------------------------------------------------------------------|-------------------------------------|
| `allow_reference_names` | Resolve the query ID if the query also contains the reference names set by this option. | Array of reference names or `'All'`                                   | `'All'`                             | 
| `allow_fields`          | Resolve the query ID if the query also contains the fields set by this option.          | Array of fields or `'All'`                                            | `'All'`                             |
| `allow_tags`            | Resolve the query ID if the query also contains the tags set by this option.            | Array of tags or `'All'`                                              | `'All'`                             |
| `allow_formats`         | Resolve the query ID if the query is one of the formats specified by this option.       | An array of formats containing `'BAM'`, `'CRAM'`, `'VCF'`, or `'BCF'` | `['BAM', 'CRAM', 'VCF', 'BCF']`     |
| `allow_classes`         | Resolve the query ID if the query is one of the classes specified by this option.       | An array of classes containing eithr `'body'` or `'header'`           | `['body', 'header']`                |
| `allow_interval.start`  | Resolve the query ID if the query reference start position is at least this option.     | Unsigned 32-bit integer start position, 0-based, inclusive            | Not set, allows all start positions |
| `allow_interval.end`    | Resolve the query ID if the query reference end position is at most this option.        | Unsigned 32-bit integer end position, 0-based exclusive               | Not set, allows all end positions   |

For example, match only if the request queries `chr1` with positions between `100` and `1000`:

```toml
[[locations]]
regex = ".*"
substitution_string = "$0"

backend.kind = "S3"
backend.bucket = "bucket"

guard.allow_reference_names = ["chr1"]
guard.allow_interval.start = 100
guard.allow_interval.end = 1000
```

### Server configuration

To use custom root certificates for `Url` locations, set the following:

```toml
[[locations]]
regex = ".*"
substitution_string = "$0"

backend.kind = "Url"
backend.url = "https://example.com"
backend.tls.root_store = "root.crt"
```

This project uses [rustls] for all TLS logic, and it does not depend on OpenSSL. The rustls library can be more
strict when accepting certificates and keys. If generating certificates for `root_store` using OpenSSL, the correct extensions,
such as `subjectAltName` should be included.

An example of generating a custom root CA and certificates for a `Url` backend:

```sh
# Create a root CA
openssl req -x509 -noenc -subj '/CN=localhost' -newkey rsa -keyout root.key -out root.crt

# Create a certificate signing request
openssl req -noenc -newkey rsa -keyout key.pem -out server.csr -subj '/CN=localhost' -addext subjectAltName=DNS:localhost

# Create the `Url` server's certificate
openssl x509 -req -in server.csr -CA root.crt -CAkey root.key -days 365 -out cert.pem -copy_extensions copy

# An additional client certificate signing request and certificate can be created in the same way as the server
# certificate if using client authentication.
```

CORS can also be configured for the data and ticket servers by specifying the `cors` option:

```toml
ticket_server.cors.allow_credentials = false
ticket_server.cors.allow_origins = "Mirror"
ticket_server.cors.allow_headers = "All"
ticket_server.cors.allow_methods = ["GET", "POST"]
ticket_server.cors.max_age = 86400
ticket_server.cors.expose_headers = []
```

Use `"Mirror"` to mirror CORS requests, and `"All"` to allow all methods, headers, or origins. The `ticket_server` table
above can be replaced with `data_server` to configure CORS for the data server.

### JWT Authorization

One advantage of the htsget protocol is that it is possible to make decisions about which regions of files a user is allowed
to access, as the protocol is able to return a subset of a genomic file in the URL tickets. Custom JWT authorization can be
configured to enable this.

htsget-rs is a stateless service (except for caching) which means that making authorization decisions can be challenging
there is no user tracking. To solve this, authorization is configured to call out to an arbitrary url to make decisions
about a user. If this feature is configured, when a JWT is sent in the authorization header, htsget-rs:

1. Decodes and validates the JWT according to the config.
2. Queries the authorization service for restrictions based on the config or JWT claims. 
3. Validates the restrictions to determine if the user is authorized.

The authorization server should respond with a rule set that htsget-rs can use to approve or deny the user access.

The following options can be configured under the `auth` table to enable this:

| Option                       | Description                                                                                                                                                                                                                                                                                                                          | Type             | Default                                                                                                |
|------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------|--------------------------------------------------------------------------------------------------------|
| `jwks_url`                   | The JSON web key sets url to fetch key sets validating the JWT token.                                                                                                                                                                                                                                                                | URL              | Not set, either this option or `decode_public_key` must be set to validate JWTs.                       | 
| `public_key`                 | The path to PEM formatted public key used to decode the JWT token.                                                                                                                                                                                                                                                                   | Filesystem path  | Not set, either this option `jwks_url` must be set to validate JWTs.                                   |
| `validate_audience`          | Validate that the JWT token has the specified audience field.                                                                                                                                                                                                                                                                        | Array of strings | Optional. Does not validate the audience by default.                                                   |
| `validate_issuer`            | Validate that the JWT token has the specified issuer field.                                                                                                                                                                                                                                                                          | Array of strings | Optional. Does not validate the issuer by default.                                                     |
| `validate_subject`           | Validate that the JWT token has the specified subject field.                                                                                                                                                                                                                                                                         | Strings          | Optional. Does not validate the subject by default.                                                    |
| `trusted_authorization_urls` | The URLs which can be called to authorize the user. If `authorization_path` is not set, the first URL in the array will be called with a GET request and the forwarded JWT. If `authorization_path` is set, then the list of URLs will be trusted as authorization sources if they are found in the JWT.                             | Array of URLs    | Not set, must be set with at least one URL.                                                            |
| `authorization_path`         | The JSON path that finds the authorization URL in the JWT. The path should find a URL value inside the JWT claims. This allows dynamically resolving the authorization url based on the content of the JWT. This URL will be called with a GET request and the forwarded JWT and should return the htsget authorization information. | JSON path        | Optional. Uses the first value in `trusted_authorization_urls` by default.                             |
| `tls`                        | Enables client authentication, or sets non-native root certificates for TLS when making requests. See [server configuration](#server-configuration) for more details.                                                                                                                                                                | TOML table       | Optional. Performs no client authentication and uses native root certificates for TLS client requests. |
| `authentication_only`        | Skip the authorization service call out and only validate the JWT token. If set, this will skip the authorization flow described above, however it will still perform JWT validation.                                                                                                                                                | Boolean          | `false`                                                                                                |

When calling the authorization service configured using `authorization_url` or `authorization_path`, htsget-rs will
forward the JWT inside an authorization bearer header and use a GET request. The service should respond with the
following JSON structure, indicating whether the request is allowed, and any region restrictions (similar to the 
[allow guard logic](#allow-guard)):

```json
{
  "version": 1,
  "htsgetAuth": [
    {
      "path": "dataset/001/id",
      "referenceNames": [
        {
          "name": "chr1",
          "format": "BAM",
          "start": 1000,
          "end": 2000
        }
      ]
    }
  ]
}
```

These restrictions act as a whitelist of allowed regions that the user has access to. The authorization server is
allowed to respond with multiple paths that the user is allowed to access. Each path can also be a regex that matches
ids like the [regex resolvers](#regex-based-location). A full JSON schema defining this format is available under [auth.json][auth-json].
An [example][auth-example] configuration file is available in the examples directory.

With this authorization logic, the server will respond with a `403 Forbidden` error if any of the requested reference
names are not allowed according to the restrictions. For example, if using the above JSON restrictions, a user
requesting `?referenceName=chr1&format=BAM&start=500&end=1500` will receive a `403` error, even though part of the range
is satisfiable (i.e. according to the restrictions, from `start=1000` to `end=1500`). In order to address this issue, the
following flag can be enabled under the `auth` table:

| Option            | Description                                                                                      | Type    | Default |
|-------------------|--------------------------------------------------------------------------------------------------|---------|---------|
| `suppress_errors` | Return any available regions according to restrictions, even if the full request is not allowed. | Boolean | `false` |
| `add_hint`        | Add a hint to the ticket response that indicates which regions the client is allowed to view.    | Boolean | `true`  |

To enable this option, htsget-rs needs to be compiled with `--features experimental` as suppressed errors lead to as
many regions as possible being returned, which may not follow the htsget protocol. When this option is used, the
above example with the request: `?referenceName=chr1&format=BAM&start=500&end=1500`, would return reads between `start=1000`
and `end=1500` for the reference name. Additionally, there will be another field present in the JSON response which hints
to clients that the full range was not satisfied because of authorization restrictions:

```json
{
  "htsget": {
    "format": "BAM",
    "urls": [<list of urls...>],
    "allowed": [
      {
        "name": "chr1",
        "format": "BAM",
        "start": 1000,
        "end": 2000
      }
    ] 
  }
}            
```

The `allowed` field echos the rule defined in the restrictions, and allows clients to plan for a partially returned
response. This field can be removed by setting `add_hint = false` in the `auth` table.

The following diagram shows how the suppressed response behaves given restrictions based on the start and end restrictions
and the requested range. The response shows the data that the user will receive.

```
Restriction:                 1000----------2000
Request:             500------------1500
Resulting response:          1000---1500

Restriction:                 1000----------2000
Request:                            1500------------2500
Resulting response:                 1500---2000

Restriction:                 1000----------2000
Request:             500-----1000
Resulting response:          empty header only
```

If `suppress_errors = true` was not used, all the above requests would result in an error instead.

Note that when using `suppress_errors = true`, when a client is not authorization to view any of the requested regions,
a non-error response is still returned with an empty response containing the file header. For example, if the
client was not authorized to view `chr1` at all, the response would return urls that correspond to a valid file
(with the file header and EOF block), but no actual reads data. Authorization error responses are still returned if the
JWT is invalid or the client is not allowed to view the requested `<id>` path. Parameters are only checked for validity
after authorizing the request, so invalid requests may return an unauthorized or forbidden response instead of a bad
request if the user lacks authorization.

[auth-json]: docs/schemas/auth.schema.json
[auth-example]: docs/examples/auth.toml

### MinIO

Operating a local object storage like [MinIO][minio] can be achieved by using `endpoint` under `"S3"` locations as shown below:

```toml
[[locations]]
regex = ".*"
substitution_string = "$0"

backend.kind = 'S3'
backend.bucket = 'bucket'
backend.endpoint = 'http://127.0.0.1:9000'
backend.path_style = true
```

Care must be taken to ensure that the [correct][env-variables] `AWS_DEFAULT_REGION`, `AWS_ACCESS_KEY` and `AWS_SECRET_ACCESS_KEY` are set to allow
the AWS sdk to reach the endpoint. Additional configuration of the MinIO server is required to use [virtual-hosted][virtual-addressing] style
addressing by setting the `MINIO_DOMAIN` environment variable. [Path][path-addressing] style addressing can be forced using `path_style = true`.

See the MinIO deployment [example][minio-deployment] for more information on how to configure htsget-rs and MinIO.

### Crypt4GH

There is experimental support for serving [Crypt4GH][c4gh] encrypted files which required compilation with `--features experimental`.

This allows htsget-rs to read Crypt4GH files and serve them encrypted, directly to the client. In the process of
serving the data, htsget-rs will decrypt the headers of the Crypt4GH files and re-encrypt them so that the client can read
them. When the client receives byte ranges from htsget-rs and concatenates them, the output bytes will be Crypt4GH encrypted,
and will need to be decrypted before they can be read. All file formats (BAM, CRAM, VCF, and BCF) are supported using Crypt4GH.

To use this feature, set `keys.kind = "File"` under the `location` table to specify the private and public keys:

| Option    | Description                                                                                                                                                                            | Type              | Default |
|-----------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------|---------|
| `private` | The path to PEM formatted private key which htsget-rs uses to decrypt Crypt4GH data.                                                                                                   | Filesystem path   | Not Set | 
| `public`  | The path to the PEM formatted public key which the recipient of the data will use. This is what the client will use to decrypt the returned data, using the corresponding private key. | Filesystem path   | Not Set |

For example:

```toml
[[locations]]
regex = ".*"
substitution_string = "$0"

backend.kind = "File"

backend.keys.kind = "File"
backend.keys.private = "data/c4gh/keys/bob.sec" # pragma: allowlist secret
backend.keys.public = "data/c4gh/keys/alice.pub"
```

Keys can also be retrieved from [AWS Secrets Manager][secrets-manager]. Compile with the `aws` feature flag and specify `keys.kind = "SecretsManager"` under
`location` to fetch keys from Secrets Manager. When using Secrets Manager, the `private` and `public`
correspond to ARNs or secret names in Secrets Manager storing PEM formatted keys.

For example:

```toml
[[locations]]
regex = ".*"
substitution_string = "$0"

backend.kind = "File"

backend.keys.kind = "SecretsManager"
backend.keys.private = "private_key_secret_name" # pragma: allowlist secret
backend.keys.public = "public_key_secret_name"
```

The htsget-rs server expects the Crypt4GH file to end with `.c4gh`, and the index file to be unencrypted. See the [`data/c4gh`][data-c4gh] for examples of file structure.
Any of the storage types are supported, i.e. `Local`, `S3`, or `Url`.

### Log formatting
 
The `RUST_LOG` variable is read to configure the level that trace logs are emitted.

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

### Environment variables

Advanced configuration options also support environment variables. Generally, options separated by `.` in a config file
are separated by `_` in the corresponding environment variable. For example, to set the ticket server allow origins,
use `HTSGET_TICKET_SERVER_CORS_ALLOW_ORIGINS`. It is not recommended to set regex-based locations using environment
variables because the variables needs to contain the nested array structure of storage backends.

### As a library

This crate reads config files and environment variables using [figment], and accepts command-line arguments using clap. The main function for this is `from_config`,
which is used to obtain the `Config` struct. The crate also contains the `resolver` abstraction, which is used for matching a query ID with
regex, and changing it by using a substitution string. Advanced configuration options are specified in the [`advanced.rs`][advanced] submodule.

[advanced]: src/config/advanced/mod.rs
[figment]: https://github.com/SergioBenitez/Figment

### Feature flags

This crate has the following features:
* `aws`: used to enable `S3` location functionality and any other AWS features.
* `url`: used to enable `Url` location functionality.
* `experimental`: used to enable experimental features that aren't necessarily part of the htsget spec, such as Crypt4GH support through `C4GHStorage`,
                  or suppressed errors when using authorization logic.

## License

This project is licensed under the [MIT license][license].

[tracing]: https://github.com/tokio-rs/tracing
[rust-log]: https://rust-lang-nursery.github.io/rust-cookbook/development_tools/debugging/config_log.html
[formatting-style]: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/fmt/index.html#formatters
[examples-config-files]: docs/examples/config-files
[rustls]: https://github.com/rustls/rustls
[htsget-actix]: ../htsget-actix
[htsget-axum]: ../htsget-axum
[htsget-lambda]: ../htsget-lambda
[tracing]: https://github.com/tokio-rs/tracing
[rust-log]: https://rust-lang-nursery.github.io/rust-cookbook/development_tools/debugging/config_log.html
[formatting-style]: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/fmt/index.html#formatters
[service-info]: https://samtools.github.io/hts-specs/htsget.html#ga4gh-service-info
[path-addressing]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/VirtualHosting.html#path-style-access
[env-variables]: https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-envvars.html
[virtual-addressing]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/VirtualHosting.html#virtual-hosted-style-access
[minio-deployment]: ../docker/examples/minio/README.md
[license]: LICENSE
[minio]: https://min.io/
[c4gh]: https://samtools.github.io/hts-specs/crypt4gh.pdf
[data-c4gh]: ../data/c4gh
[secrets-manager]: https://docs.aws.amazon.com/secretsmanager/latest/userguide/intro.html
[id]: https://samtools.github.io/hts-specs/htsget.html#url-parameters
[toml]: https://toml.io/en/
[data]: ../data