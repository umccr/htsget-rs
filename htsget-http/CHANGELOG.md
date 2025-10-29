# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.0](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.7.4...htsget-http-v0.8.0) - 2025-10-27

### Added

- *(http)* implement the forward id logic
- *(http)* implement forward extension type logic
- [**breaking**] use http client config before constructing it in the builder
- use config user-agent for call-outs to authorization server
- *(config)* [**breaking**] add user agent option for http clients and add alias for location in response

## [0.7.4](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.7.3...htsget-http-v0.7.4) - 2025-09-25

### Added

- use prefix, id or regex based location to match authorization
- *(config)* add dynamic location option for authorization rules
- implement extension forwarding logic from Lambda events
- implement header forwarding logic
- implement new config options and refactor code for already used options
- *(config)* start splitting out JWT authorization/authentication

### Other

- regenerate docs and adjust json schema definition
- update tests based on new auth config
- Merge branch 'main' of https://github.com/umccr/htsget-rs into feat/generalized-auth

## [0.7.3](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.7.2...htsget-http-v0.7.3) - 2025-09-03

### Added

- add auth logic to post requests and always allow headers to succeed
- *(http)* implement suppressing errors in http middleware and get/post routes

### Other

- add more robust start/end range tests and document suppressed errors with diagrams.
- add header query authorization test

## [0.7.2](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.7.1...htsget-http-v0.7.2) - 2025-08-21

### Other

- updated the following local packages: htsget-config, htsget-search

## [0.7.1](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.7.0...htsget-http-v0.7.1) - 2025-08-15

### Fixed

- data server should forward JWT token if auth is used

### Other

- remove duplicate changelog section
- Merge pull request #320 from umccr/feat/auth

## [0.7.0](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.6.0...htsget-http-v0.7.0) - 2025-08-11

### Added

- [**breaking**] extract authorize request function and correct validate service-info routes and authorize data routes

### Fixed

- *(http)* forward JWT token to authorization service
- *(http)* auth token extraction must iterate all header values as the decode iterator only parses the next value in the iterator

### Other

- Merge pull request #315 from umccr/feat/auth

## [0.6.0](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.5.5...htsget-http-v0.6.0) - 2025-08-08

### Added

- add authentication_only option
- add validate jwt only function
- add builder for auth config and response and validate fields
- [**breaking**] update editions, unused dependencies and newly unsafe set_var in Lambda function
- add restriction checks to authorization flow for htsget servers
- *(http)* add authorization checks based on service restrictions
- *(http)* export modules
- *(http)* move auth logic to htsget-http

### Other

- key_pair simplification, doc updates, response default values and well-known JWKS updates
- add integration tests for JWT auth

## [0.5.5](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.5.4...htsget-http-v0.5.5) - 2025-07-22

### Other

- updated the following local packages: htsget-config, htsget-search

## [0.5.4](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.5.3...htsget-http-v0.5.4) - 2025-04-28

### Other

- updated the following local packages: htsget-config, htsget-search

## [0.5.3](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.5.2...htsget-http-v0.5.3) - 2025-02-18

### Other

- *(htsget-config)* release v0.13.1

## [0.5.2](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.5.1...htsget-http-v0.5.2) - 2025-01-24

### Added

- add encryption scheme to http and config crates
- *(config)* add cargo package filled service info fields

### Fixed

- *(http)* allow encryption scheme to be uppercase
- service info group, artifact and version, and add flexibility in configuration

### Other

- rename s3-storage to aws and url-storage to url
- add location concept and move advanced config to its own module
- re-word and simplify, add quick starts where applicable

## [0.5.1](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.5.0...htsget-http-v0.5.1) - 2024-10-16

### Other

- updated the following local packages: htsget-config, htsget-search, htsget-test

## [0.5.0](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.16...htsget-http-v0.5.0) - 2024-09-30

### Added

- *(config)* add configuration values for C4GH S3 and Url storage
- Crypt4GH support using LocalStorage

### Other

- remove `ObjectType` in favour of a more flattened config hierarchy
- rename type to backend and clarify experimental feature flag
- [**breaking**] make storage config more explicit
- [**breaking**] rename c4gh-experimental to experimental
- [**breaking**] allow mutable search trait, use way less Arcs and clones

## [0.4.16](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.15...htsget-http-v0.4.16) - 2024-09-03

### Other
- updated the following local packages: htsget-search

## [0.4.15](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.14...htsget-http-v0.4.15) - 2024-08-04

### Other
- update rust msrv
- move storage module from htsget-search to htsget-storage

## [0.4.14](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.13...htsget-http-v0.4.14) - 2024-05-22

### Other
- major dep updates, rustls 0.21 -> 0.23, http 0.2 -> 1, reqwest 0.11 -> 0.12, noodles 0.65 -> 0.74 + minor bumps for other crates depending on these.
- Merge branch 'main' of https://github.com/umccr/htsget-rs into mio_h2_bump

## [0.4.13](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.12...htsget-http-v0.4.13) - 2024-05-19

### Other
- update MSRV

## [0.4.12](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.11...htsget-http-v0.4.12) - 2024-01-02

### Other
- updated the following local packages: htsget-config, htsget-search, htsget-test

## [0.4.11](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.10...htsget-http-v0.4.11) - 2023-11-02

### Other
- updated the following local packages: htsget-config, htsget-search

## [0.4.10](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.9...htsget-http-v0.4.10) - 2023-10-30

### Other
- updated the following local packages: htsget-search

## [0.4.9](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.8...htsget-http-v0.4.9) - 2023-10-02

### Other
- updated the following local packages: htsget-search

## [0.4.8](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.7...htsget-http-v0.4.8) - 2023-09-06

### Other
- revert htsget-test change to a dev dependency
- add pre-commit hook
- *(deps)* update msrv and attempt using htsget-test as a dev dependency

## [0.4.7](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.6...htsget-http-v0.4.7) - 2023-09-05

### Other
- updated the following local packages: htsget-test

## [0.4.6](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.5...htsget-http-v0.4.6) - 2023-07-11

### Other
- updated the following local packages: htsget-config, htsget-search, htsget-test

## [0.4.5](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.4...htsget-http-v0.4.5) - 2023-06-25

### Other
- updated the following local packages: htsget-search

## [0.4.4](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.3...htsget-http-v0.4.4) - 2023-06-20

### Other
- updated the following local packages: htsget-config, htsget-search, htsget-test

## [0.4.3](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.2...htsget-http-v0.4.3) - 2023-06-19

### Other
- updated the following local packages: htsget-config, htsget-search

## [0.4.2](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.1...htsget-http-v0.4.2) - 2023-06-08

### Other
- remove unused dependencies and update msrv
- update remaining dependencies, hold back tokio-rustls due to conflicting versions with actix-web

## [0.4.1](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.4.0...htsget-http-v0.4.1) - 2023-06-02

### Other
- updated the following local packages: htsget-config

## [0.3.0](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.2.0...htsget-http-v0.3.0) - 2023-05-29

### Added
- format parsing is now case-insensitive when validating query parameters
- [**breaking**] add request header information to post handlers
- [**breaking**] add request header information to get handlers
- *(config)* add url-storage feature flag
- add error type to config
- *(config)* add automatic config inference for local and s3 storage, and rearrange modules
- [**breaking**] simplify storage config by allowing untagged enum representation

### Fixed
- [**breaking**] headers should allow multiple values for the same key
- *(http)* return InvalidInput when query parameters are present for a post request
- use correct help context for a crate using htsget-config
- *(release)* Bump all crates to 0.1.2 as explored in https://github.com/MarcoIeni/release-plz/issues/507#issuecomment-1422187766

### Other
- update for UrlStorage
- [**breaking**] rename AwsS3Storage to S3Storage in search
- [**breaking**] http refactor, pass request with query
- remove s3-storage as default
- Merge branch 'main' of https://github.com/umccr/htsget-rs into feat/actix-tls
- [**breaking**] rename `HttpTicketFormatter` and remove `UrlFormatter` implementation for it
- [**breaking**] move htsget structs to config, and resolve storage type in config
- manually bump crate versions to 0.1.4
- make htsget-test a regular dependency
- bump crate versions to 0.1.3 manually
- specify htsget-test version
- *(http)* make naming of service info fields consistent
- release
- Downgrade release after fixing cargo publish circular dep issues as discussed in https://github.com/MarcoIeni/release-plz/issues/507#issuecomment-1420254400
- Update cargo files ([#152](https://github.com/umccr/htsget-rs/pull/152))
- *(deps)* bump tokio from 1.24.0 to 1.24.2 ([#151](https://github.com/umccr/htsget-rs/pull/151))
- release ([#148](https://github.com/umccr/htsget-rs/pull/148))
- Set MSRV on all sub-crates ([#146](https://github.com/umccr/htsget-rs/pull/146))
- Better CI ([#98](https://github.com/umccr/htsget-rs/pull/98))

## [0.2.0](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.1.4...htsget-http-v0.2.0) - 2023-04-28

### Added
- *(config)* add automatic config inference for local and s3 storage, and rearrange modules
- [**breaking**] simplify storage config by allowing untagged enum representation

### Other
- [**breaking**] rename `HttpTicketFormatter` and remove `UrlFormatter` implementation for it
- [**breaking**] move htsget structs to config, and resolve storage type in config

## [0.1.1](https://github.com/umccr/htsget-rs/compare/htsget-http-v0.1.0...htsget-http-v0.1.1) - 2023-01-27

### Other
- Set MSRV on all sub-crates (#146)
- Better CI (#98)
