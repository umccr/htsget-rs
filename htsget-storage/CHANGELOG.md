# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.1](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.5.0...htsget-storage-v0.5.1) - 2025-10-27

### Added

- use config user-agent for call-outs to authorization server

## [0.5.0](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.4.4...htsget-storage-v0.5.0) - 2025-09-25

### Added

- [**breaking**] add on-disk caching layer to all client requests within the server

## [0.4.4](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.4.3...htsget-storage-v0.4.4) - 2025-09-03

### Other

- updated the following local packages: htsget-config

## [0.4.3](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.4.2...htsget-storage-v0.4.3) - 2025-08-21

### Other

- updated the following local packages: htsget-config

## [0.4.2](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.4.1...htsget-storage-v0.4.2) - 2025-08-15

### Added

- add ticket_origin option to override the ticket values produced when pointing to the data server

### Fixed

- data server should forward JWT token if auth is used

### Other

- remove duplicate changelog section
- Merge pull request #320 from umccr/feat/auth

## [0.4.1](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.4.0...htsget-storage-v0.4.1) - 2025-08-11

### Other

- updated the following local packages: htsget-config

## [0.4.0](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.3.3...htsget-storage-v0.4.0) - 2025-08-08

### Added

- add authentication_only option
- [**breaking**] update editions, unused dependencies and newly unsafe set_var in Lambda function

## [0.3.3](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.3.2...htsget-storage-v0.3.3) - 2025-07-22

### Other

- update pre-commit versions
- update dependencies and format

## [0.3.2](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.3.1...htsget-storage-v0.3.2) - 2025-04-28

### Other

- *(deps)* Back pedal on bincode version bump 1->2 due to ::serialize conflicts with crypt4gh... that method is contested on the crypt4gh-rust crate implementation anyway, so we might not even use that serialization approach in the (near?) future anyway.
- *(deps)* Noodles 0.97
- *(fix)* Apply iterator optimization according to clippy: 'error: called Iterator::last on a DoubleEndedIterator; this will needlessly iterate the entire iterator'

## [0.3.1](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.3.0...htsget-storage-v0.3.1) - 2025-02-18

### Other

- *(htsget-config)* release v0.13.1

## [0.3.0](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.2.1...htsget-storage-v0.3.0) - 2025-01-24

### Added

- [**breaking**] implement encryption scheme logic in storage
- *(storage)* add unsupported format to storage error and tidy message
- *(config)* add cargo package filled service info fields

### Other

- *(storage)* reword error message
- add test for encryption scheme flag
- rename s3-storage to aws and url-storage to url
- add location concept and move advanced config to its own module
- re-word and simplify, add quick starts where applicable

## [0.2.1](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.2.0...htsget-storage-v0.2.1) - 2024-10-16

### Added

- allow retrieving c4gh keys from secrets manager

### Fixed

- *(storage)* ensure c4gh data is read after determining the header size

### Other

- *(search)* remove unnecessary Arcs and unwraps in test code

## [0.2.0](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.1.1...htsget-storage-v0.2.0) - 2024-09-30

### Added

- *(storage)* allow S3 and Url storage to use C4GHStorage
- *(config)* add configuration values for C4GH S3 and Url storage
- Crypt4GH support using LocalStorage

### Fixed

- *(storage)* make overflow handling more robust
- build errors with conditional flags

### Other

- *(deps)* bump noodles and tower
- remove `ObjectType` in favour of a more flattened config hierarchy
- rename type to backend and clarify experimental feature flag
- *(config)* slight adjustment to reduce conditionally compiled code branches
- [**breaking**] make storage config more explicit
- *(config)* [**breaking**] remove object type and specify keys directly
- [**breaking**] rename c4gh-experimental to experimental
- [**breaking**] split out storage lib.rs into another types.rs module
- *(storage)* add c4gh storage tests
- *(storage)* use preprocess and postprocess for C4GH storage instead of Arc<Mutex<..>>
- [**breaking**] allow mutable search trait, use way less Arcs and clones

## [0.1.1](https://github.com/umccr/htsget-rs/compare/htsget-storage-v0.1.0...htsget-storage-v0.1.1) - 2024-08-05

### Other
- release

## [0.1.0](https://github.com/umccr/htsget-rs/releases/tag/htsget-storage-v0.1.0) - 2024-08-04

### Other
- update rust msrv
- add routers for data and ticket servers
- move the data server to its own htsget-axum crate
- move storage module from htsget-search to htsget-storage
