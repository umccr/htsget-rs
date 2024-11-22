# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.2.1...htsget-axum-v0.2.2) - 2024-10-22

### Other

- update Cargo.lock dependencies

## [0.2.1](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.2.0...htsget-axum-v0.2.1) - 2024-10-16

### Other

- update Cargo.lock dependencies

## [0.2.0](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.1.1...htsget-axum-v0.2.0) - 2024-09-30

### Added

- *(config)* add configuration values for C4GH S3 and Url storage
- Crypt4GH support using LocalStorage

### Fixed

- explicitly choose aws_lc_rs as the crypto provider

### Other

- *(deps)* bump noodles and tower
- rename type to backend and clarify experimental feature flag
- bump noodles and other dependencies
- [**breaking**] rename c4gh-experimental to experimental
- [**breaking**] allow mutable search trait, use way less Arcs and clones
- add example on how to use Crypt4GH with the htsget-rs server

## [0.1.1](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.1.0...htsget-axum-v0.1.1) - 2024-09-03

### Other
- release
- release

## [0.1.0](https://github.com/umccr/htsget-rs/releases/tag/htsget-axum-v0.1.0) - 2024-09-03

### Added
- *(axum)* add join handle helper functions

### Fixed
- *(axum)* enable http2 support, re-word docs to include htsget-axum

### Other
- release
- release
- update rust msrv
- *(axum)* add server tests for axum ticket server
- add routers for data and ticket servers
- move the data server to its own htsget-axum crate

## [0.1.0](https://github.com/umccr/htsget-rs/releases/tag/htsget-axum-v0.1.0) - 2024-08-05

### Added
- *(axum)* add join handle helper functions

### Fixed
- *(axum)* enable http2 support, re-word docs to include htsget-axum

### Other
- release
- update rust msrv
- *(axum)* add server tests for axum ticket server
- add routers for data and ticket servers
- move the data server to its own htsget-axum crate

## [0.1.0](https://github.com/umccr/htsget-rs/releases/tag/htsget-axum-v0.1.0) - 2024-08-04

### Added
- *(axum)* add join handle helper functions

### Fixed
- *(axum)* enable http2 support, re-word docs to include htsget-axum

### Other
- update rust msrv
- *(axum)* add server tests for axum ticket server
- add routers for data and ticket servers
- move the data server to its own htsget-axum crate
