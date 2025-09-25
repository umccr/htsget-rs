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
curl 'http://localhost:8080/reads/bam/htsnexus_test_NA12878'
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

For any of the above examples, htsget-rs will look for data using the id passed in the request. Locations can
also have extra path segments to lookup nested data. For example, when using `location = "s3://bucket/dir"`,
and the request `/reads/bam/htsnexus_test_NA12878`,  htsget-rs will search for data under
`s3://bucket/dir/bam/htsnexus_test_NA12878.bam` and `s3://bucket/dir/bam/htsnexus_test_NA12878.bam.bai`,
and return tickets for `s3://bucket/dir/bam/htsnexus_test_NA12878.bam`.

> [!IMPORTANT]  
> The file extension of data should not be specified in the request id and location. Any request for data will always
> look for files with the file format extension. For example, a bam file will always use `.bam` and a VCF file will
> always use `.vcf.gz`.

Multiple locations can be specified by providing a list and an prefix after the location:

```toml
locations = [ { location = "file://data", prefix = "bam" }, { location = "s3://bucket", prefix = "cram" } ]
```

This allows htsget-rs to serve data only when the request also contains the prefix. For example, 
`/reads/bam/htsnexus_test_NA12878` will go to `file://data/bam/htsnexus_test_NA12878.bam` and
`/reads/cram/htsnexus_test_NA12878?format=CRAM'` will go to `s3://bucket/cram/htsnexus_test_NA12878.cram`.

When specifying locations like this, the location is additive. That is, the request id is appended to the
location. This means that when the user requests data at `/reads/<id>`, the server fetches data and returns tickets
from `<location>/<id>`. As an alternative to additive locations, a location can be specified as an exact match for a file by setting the id
field:

```toml
locations = [ { location = "file://data/file", id = "bam_file" }, { location = "s3://bucket/file", id = "cram_file" } ]
```

Now, when a user requests data at `/reads/bam_file`, the server will use `file://data/file.bam` and
`file://data/file.bam.bai` instead of appending the id, and similar for `/reads/cram_file`. The advantage of this is 
that it decouples the requested id from the name of the file completely. In general, when using exact id matches,
a request for data at `/reads/<id>` will result in fetching data and returning tickets from `<location>`.

For added flexibility, there is an alternative location configuration system using regex under [advanced config].

> [!IMPORTANT]  
> Some parts of htsget-rs require extra feature flags for conditional compilation, that's why the examples specify
> using `--all-features`. Notably, `--features aws` enables the `S3` location type, and `--features url`
> enabled the remote HTTP server location type. If using a subset of features, for example S3 locations only, then
> a single feature can be enabled instead of using `--all-features`.

### Server config

htsget-rs spawn up to two server instances - the ticket server, which responds to the initial htsget request, and
optionally, the data server, which responds to the htsget tickets if using file locations.

The data server's path can be set by using `data_server.local_path`. When using `file://<directory>` locations, the
directory component must be the same as the local path so that the server has access to it. It is also not possible to
have different directories components when using multiple `file://<directory>` locations. This means that there can only
be one file location directory, although there can be multiple matching prefixes or ids to control the request.

The data server process can be disabled by setting it to `None` if no file locations are being used:

```toml
data_server = "None"
```

This is automatically applied if no file locations are configured.

By default, file locations specified via `file://<dir>` will use the data server scheme and
address for ticket responses. This means that tickets will be served as `<scheme>://<addr>/<id>`,
pointing to the data server `<scheme>` and `<addr>` automatically. For example, a default `file://data`
location will have tickets that look like `http://127.0.0.1:8081/<id>`.

The scheme and address can be overridden for any file-based responses by setting `data_server.ticket_origin`:

```toml
# Ensure that the scheme is set for the origin.
data_server.ticket_origin = "https://example.com/"
## The url can also have a path set, which appears in the tickets.
#data_server.ticket_origin = "https://example.com/path"
```

In this example, the tickets will appear as `https://example.com/<id>`. This is useful to arbitrarily route tickets to
DNS-resolvable requests, for example, inside a docker container.

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

| Option                               | Description                                                                                                                                                                                    | Type                     | Default                                                                                                         |
|--------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------|-----------------------------------------------------------------------------------------------------------------|
| <span id="url">`url`</span>          | The URL to fetch data from.                                                                                                                                                                    | HTTP URL                 | `"https://127.0.0.1:8081/"`                                                                                     |
| <span id="url">`response_url`</span> | The URL to return to the client for fetching tickets.                                                                                                                                          | HTTP URL                 | `"https://127.0.0.1:8081/"`                                                                                     |
| `forward_headers`                    | When constructing the URL tickets, copy HTTP headers received in the initial query.                                                                                                            | Boolean                  | `true`                                                                                                          |
| `header_blacklist`                   | List of headers that should not be forwarded.                                                                                                                                                  | Array of headers         | `[]`                                                                                                            |
| `http`                               | Additionally enables client authentication, or sets non-native root certificates for TLS, or disables HTTP header caching. See [server configuration](#server-configuration) for more details. | TOML table               | TLS is always allowed, however the default performs no client authentication and uses native root certificates. |

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

> [!NOTE]  
> Calls to the url endpoint use HTTP caching from headers like cache-control and expires. Requests to the url endpoint
> only involve the beginning of a file to obtain the header. These requests are cached in a temporary file called
> `htsget_rs_client_cache` in the system temp directory. To disable this functionality set
> `http.use_cache = false`.

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
| `allow_classes`         | Resolve the query ID if the query is one of the classes specified by this option.       | An array of classes containing either `'body'` or `'header'`          | `['body', 'header']`                |
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
backend.http.root_store = "root.crt"
## Disable HTTP caching.
#backend.http.use_cache = false
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

### JWT Authentication

The htsget-rs ticket and data servers can be configured to validate and authenticate JWT tokens.

The following options can be configured under the `auth` table to enable this:

| Option                  | Description                                                                                                                                                                                      | Type             | Default                                                                                                           |
|-------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------|-------------------------------------------------------------------------------------------------------------------|
| `jwks_url`              | The JSON web key sets url to fetch key sets validating the JWT token.                                                                                                                            | URL              | Not set, either this option or `decode_public_key` must be set to validate JWTs.                                  | 
| `public_key`            | The path to PEM formatted public key used to decode the JWT token.                                                                                                                               | Filesystem path  | Not set, either this option `jwks_url` must be set to validate JWTs.                                              |
| `validate_audience`     | Validate that the JWT token has the specified audience field.                                                                                                                                    | Array of strings | Optional. Does not validate the audience by default.                                                              |
| `validate_issuer`       | Validate that the JWT token has the specified issuer field.                                                                                                                                      | Array of strings | Optional. Does not validate the issuer by default.                                                                |
| `validate_subject`      | Validate that the JWT token has the specified subject field.                                                                                                                                     | Strings          | Optional. Does not validate the subject by default.                                                               |
| `http`                  | Additionally enables client authentication, or sets non-native root certificates for TLS, or disables HTTP header caching. See [server configuration](#server-configuration) for more details.   | TOML table       | TLS is always allowed, however the default performs no client authentication and uses native root certificates.   |

When JWT authentication is enabled, either `jwks_url` or `public_key` must be set to validate the JWT. The `auth` table
can be set under the `data_server` or `ticket_server` table, or globally to use the same configuration for both. 
See the [example][auth-example] configuration file in the example directory.

#### Authorization

One advantage of the htsget protocol is that it is possible to make decisions about which regions of files a user is allowed
to access, as the protocol is able to return a subset of a genomic file in the URL tickets. Custom authorization can be
configured to enable this.

htsget-rs is a stateless service (except for caching) which means that making authorization decisions can be challenging
there is no user tracking. To solve this, authorization is configured to call out to an arbitrary url to make decisions
about a user. If this feature is configured, htsget-rs:

1. Decodes and validates a JWT configured above.
2. Queries the authorization service for restrictions based on the config. 
3. Validates the restrictions to determine if the user is authorized.

The authorization server should respond with a rule set that htsget-rs can use to approve or deny the user access.

The following additional options can be configured under the `auth` table to enable this:

| Option              | Description                                                                                                                                                                                                    | Type                  | Default  |
|---------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------------------|----------|
| `authorization_url` | The URL which will be called to authorize the user. A GET request will be issued to the url. Alternatively, this can be a file path to authorize users based on static config.                                 | URL                   | Not set. |
| `forward_headers`   | For each header specified, forward any headers from the client to the authorization server. Headers are forwarded with the `Htsget-Context-` as a prefix.                                                      | Array of header names | Not set. |
| `passthrough_auth`  | Forward the authorization header to the authorization server directly without renaming it to a `Htsget-Context-` custom header. If this is true, then the `Authorization` header is required with the request. | Boolean               | `false`  |

When using the `authorization_url`, the [authentication](#jwt-authentication) config must also be set as htsget-rs will
forward the JWT token to the authorization server so that it can make decisions about the user's authorization. If the
`authorization_url` is a file path, then authentication doesn't need to be set.

Each header in the `forward_headers` option is forwarded as a custom `Htsget-Context-<name>` header to the authorization server.
The authorization header can be forward as though it is coming from the client by setting `forward_auth_header = true`. This is
useful to support authenticating the original client JWT at the authorization server and can be used to set-up authorization
flows like oauth.

The authorization service should respond with the following JSON structure, indicating whether the request is allowed,
and any region restrictions:

```json
{
  "version": 1,
  "htsgetAuth": [
    {
      "id": "dataset/001/id",
      "rules": [
        {
          "referenceName": "chr1",
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
allowed to respond with multiple paths that the user is allowed to access. A full JSON schema defining this format is
available under [auth.json][auth-json].

Each auth rule can also contain a location that has the same options as the [locations config](#quickstart). This gives
the authorization server flexibility to specify locations dynamically on a per-request basis.

For example, to specify a dynamic location for VCF files separately to BAM files:

```json
{
  "version": 1,
  "htsgetAuth": [
    {
      "id": "dataset/001/id-bam",
      "location": "s3://bucket-a/bam_file",
      "rules": [
        {
          "format": "BAM"
        }
      ]
    },
    {
      "id": "dataset/001/id-vcf",
      "location": "s3://bucket-b/vcf_file",
      "rules": [
        {
          "format": "VCF"
        }
      ]
    }
  ]
}
```

Similarly to the config, prefixes can be used instead of an "id", or a full [regex location](#regex-based-location) can
be used.

> [!NOTE]  
> Calls to both the authorization service and jwks endpoint correctly handle and support HTTP cache headers like
> cache-control and expires. Requests are cached in a temporary file called `htsget_rs_client_cache` in the system
> temp directory. To disable this functionality set `http.use_cache = false`.

#### Suppressed errors

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

#### Extensions

> [!NOTE]  
> The extension options should not be used unless developing custom extensions to the htsget-rs codebase to support
> additional authorization capabilities. The options below are only useful to add out-of-band context to requests such
> as from AWS Lambda events. When using the `htsget-lambda` crate, this option is available for all Lambda event fields.
>
> This can also be used for non-Lambda implementations if developing custom middleware to use with htsget-rs routers.

As part the Rust [http][http] library, extensions can be added to requests before they are processed by the HTTP router.
This is useful to add context to requests from external sources, such as AWS ApiGateway or VPC lattice Lambda events.
These context fields can be forwarded to the authorization service to make authorization decisions about a user.

Set the following in the `auth` table to use this feature:

| Option               | Description                                                                                                                                                                                                                                                                                  | Type                              | Default   |
|----------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------|-----------|
| `forward_extensions` | For each request extension specified, forward the HTTP extension to the authorization server. This can be a full JSON path to forward nested values. Extensions are forwarded as custom `Htsget-Context-<name>` headers, where each JSON path value must be assigned a name in this setting. | Array of name and JSON path pairs | Not set.  |

For example, to forward the request context source VPC from a Lambda function handling [VPC lattice events](https://docs.aws.amazon.com/vpc-lattice/latest/ug/lambda-functions.html#receive-event-from-service), use the following
setting:

```toml
[auth]
forward_extensions = [ { json_path = '$.requestContext.identity.sourceVpcArn', name = 'SourceVpcArn'} ]
```

This would then forward the source VPC ARN to the authorization server in a header called `Htsget-Context-SourceVpcArn`.

An example of this kind of implementation can be seen [here](https://github.com/umccr/htsget-deploy/tree/main/aws-vpc-lattice).

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
[http]: https://docs.rs/http/latest/http/struct.Extensions.html