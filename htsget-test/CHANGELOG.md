# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.2](https://github.com/umccr/htsget-rs/compare/htsget-test-v0.5.1...htsget-test-v0.5.2) - 2023-09-06

### Fixed
- *(test)* http-tests feature should include htsget-config dependency

### Other
- *(deps)* properly update noodles and aws dependencies
- *(deps)* update msrv and attempt using htsget-test as a dev dependency
- bump deps including rustls 0.21
- Merge branch 'main' of https://github.com/umccr/htsget-rs into feat/htsget-elsa

## [0.5.1](https://github.com/umccr/htsget-rs/compare/htsget-test-v0.5.0...htsget-test-v0.5.1) - 2023-09-05

### Other
- bump up deploy packages, also solves CVE-2023-35165 and CVE-2022-25883

## [0.5.0](https://github.com/umccr/htsget-rs/compare/htsget-test-v0.4.4...htsget-test-v0.5.0) - 2023-07-11

### Added
- [**breaking**] implement client tls config
- [**breaking**] add server config to certificate key pair
- [**breaking**] add stronger types for certificate key pairs
- introduce cert and key parsing into config

## [0.4.4](https://github.com/umccr/htsget-rs/compare/htsget-test-v0.4.3...htsget-test-v0.4.4) - 2023-06-20

### Other
- bump deps

## [0.4.3](https://github.com/umccr/htsget-rs/compare/htsget-test-v0.4.2...htsget-test-v0.4.3) - 2023-06-19

### Other
- updated the following local packages: htsget-config

## [0.4.2](https://github.com/umccr/htsget-rs/compare/htsget-test-v0.4.1...htsget-test-v0.4.2) - 2023-06-08

### Other
- remove unused dependencies and update msrv
- update remaining dependencies, hold back tokio-rustls due to conflicting versions with actix-web
- update noodles, remove repeated code in search

## [0.4.1](https://github.com/umccr/htsget-rs/compare/htsget-test-v0.4.0...htsget-test-v0.4.1) - 2023-06-02

### Other
- updated the following local packages: htsget-config

## [0.3.0](https://github.com/umccr/htsget-rs/compare/htsget-test-v0.2.0...htsget-test-v0.3.0) - 2023-05-29

### Added
- *(config)* add url-storage feature flag
- add option to format logs in different styles
- add error type to config
- [**breaking**] add tls config to ticket server, rearrange some fields
- *(test)* add multiple resolvers for server tests and test resolution
- use serve_at in data server rather than a constant
- *(config)* add automatic config inference for local and s3 storage, and rearrange modules
- [**breaking**] simplify storage config by allowing untagged enum representation

### Fixed
- inserting keys with the same name multiple times into headers serializes correctly
- [**breaking**] headers should allow multiple values for the same key
- use correct help context for a crate using htsget-config
- *(release)* Bump all crates to 0.1.2 as explored in https://github.com/MarcoIeni/release-plz/issues/507#issuecomment-1422187766

### Other
- update for UrlStorage
- [**breaking**] rename AwsS3Storage to S3Storage in search
- remove s3-storage as default
- Merge branch 'main' of https://github.com/umccr/htsget-rs into feat/actix-tls
- [**breaking**] move htsget structs to config, and resolve storage type in config
- manually bump crate versions to 0.1.4
- bump crate versions to 0.1.3 manually
- allow htsget-test to be published and bump deps
- *(test)* remove htsget-test dependence on htsget-search and htsget-http.
- [**breaking**] move CertificateKeyPair to config to simplify data server logic
- release
- Downgrade release after fixing cargo publish circular dep issues as discussed in https://github.com/MarcoIeni/release-plz/issues/507#issuecomment-1420254400
- Update cargo files ([#152](https://github.com/umccr/htsget-rs/pull/152))
- *(deps)* bump tokio from 1.24.0 to 1.24.2 ([#151](https://github.com/umccr/htsget-rs/pull/151))
- *(fix)* Remove version from htsget-test and mark it for publish=false to avoid circular dependency as recommended by @Marcoleni in https://github.com/MarcoIeni/release-plz/pull/452#issuecomment-1409835221
- release ([#148](https://github.com/umccr/htsget-rs/pull/148))
- Set MSRV on all sub-crates ([#146](https://github.com/umccr/htsget-rs/pull/146))
- Better CI ([#98](https://github.com/umccr/htsget-rs/pull/98))

## [0.2.0](https://github.com/umccr/htsget-rs/compare/htsget-test-v0.1.4...htsget-test-v0.2.0) - 2023-04-28

### Added
- *(test)* add multiple resolvers for server tests and test resolution
- use serve_at in data server rather than a constant
- *(config)* add automatic config inference for local and s3 storage, and rearrange modules
- [**breaking**] simplify storage config by allowing untagged enum representation

### Other
- [**breaking**] move htsget structs to config, and resolve storage type in config

## [0.1.1](https://github.com/umccr/htsget-rs/compare/htsget-test-v0.1.0...htsget-test-v0.1.1) - 2023-01-27

### Other
- Set MSRV on all sub-crates (#146)
