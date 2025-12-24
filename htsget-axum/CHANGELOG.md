# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.4](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.8.3...htsget-axum-v0.8.4) - 2025-12-24

### Other

- remove documentation to have it automatically point to crate docs

## [0.8.3](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.8.2...htsget-axum-v0.8.3) - 2025-12-03

### Other

- update Cargo.lock dependencies

## [0.8.2](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.8.1...htsget-axum-v0.8.2) - 2025-12-01

### Other

- update dependencies, clippy warnings

## [0.8.1](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.8.0...htsget-axum-v0.8.1) - 2025-10-29

### Added

- *(http)* forward error context and code with better message

### Fixed

- *(http)* authorization and authentication should be independent
- *(http)* add hint should be separate to remote location search

### Other

- Merge pull request #341 from umccr/fix/test-elsa-integration
- remove debug statements and revert lambda http dependency

## [0.8.0](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.7.0...htsget-axum-v0.8.0) - 2025-10-27

### Added

- [**breaking**] use http client config before constructing it in the builder
- use config user-agent for call-outs to authorization server

## [0.7.0](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.6.2...htsget-axum-v0.7.0) - 2025-09-25

### Added

- implement extension forwarding logic from Lambda events
- implement header forwarding logic
- [**breaking**] rename tls to http for client config

### Other

- add integration tests for prefix, id and regex

## [0.6.2](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.6.1...htsget-axum-v0.6.2) - 2025-09-03

### Added

- add auth logic to post requests and always allow headers to succeed
- add suppressed errors options to axum and actix ticket servers

### Fixed

- cors layer should run before auth to handle OPTIONS requests without authentication

### Other

- add more robust start/end range tests and document suppressed errors with diagrams.

## [0.6.1](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.6.0...htsget-axum-v0.6.1) - 2025-08-21

### Other

- update Cargo.lock dependencies

## [0.6.0](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.5.0...htsget-axum-v0.6.0) - 2025-08-15

### Fixed

- [**breaking**] add restrictions to file locations and ensure that the data server local path lines up as expected

### Other

- *(axum)* remove duplicate 0.1.0 entries
- remove duplicate changelog section
- Merge pull request #320 from umccr/feat/auth

## [0.5.0](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.4.0...htsget-axum-v0.5.0) - 2025-08-11

### Added

- [**breaking**] extract authorize request function and correct validate service-info routes and authorize data routes

### Fixed

- fallback route should respond with JSON, and add missing fallback route to actix router
- axum CORS supports both mirror and any for CORS headers, actix only supports any for origins

### Other

- Merge pull request #315 from umccr/feat/auth

## [0.4.0](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.3.3...htsget-axum-v0.4.0) - 2025-08-08

### Added

- [**breaking**] update editions, unused dependencies and newly unsafe set_var in Lambda function
- add restriction checks to authorization flow for htsget servers
- propagate config settings and validate all JWT fields in axum and actix servers
- *(axum)* implement call-out to authorization service in middleware
- propagate auth config to all server instances
- *(config)* add option to validate sub
- *(config)* use schemars to create json schema for response
- *(config)* add auth config module
- add authentication flow following jwks
- add auth middleware service and layer structs
- add implementations for creating htsget errors for htsget axum
- add into_response implementation for htsget error type

### Other

- key_pair simplification, doc updates, response default values and well-known JWKS updates
- add integration tests for JWT auth
- describe auth protocol in config docs

## [0.3.3](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.3.2...htsget-axum-v0.3.3) - 2025-07-22

### Other

- update dependencies and format

## [0.3.2](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.3.1...htsget-axum-v0.3.2) - 2025-04-28

### Other

- update Cargo.lock dependencies

## [0.3.1](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.3.0...htsget-axum-v0.3.1) - 2025-02-18

### Other

- *(htsget-config)* release v0.13.1

## [0.3.0](https://github.com/umccr/htsget-rs/compare/htsget-axum-v0.2.2...htsget-axum-v0.3.0) - 2025-01-24

### Added

- *(config)* add cargo package filled service info fields
- [**breaking**] add pre-filled package info, description and repository url to the service info endpoint

### Fixed

- service info group, artifact and version, and add flexibility in configuration

### Other

- rename s3-storage to aws and url-storage to url
- add location concept and move advanced config to its own module
- grammar and typos
- re-word and simplify, add quick starts where applicable

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
