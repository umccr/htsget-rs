# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
