# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/umccr/htsget-rs/compare/htsget-config-v0.1.0...htsget-config-v0.1.1) - 2023-01-27

### Other
- Set MSRV on all sub-crates (#146)
- Better CI (#98)
- add missing environment variable options
- remove unnecessary default column for environment variables, surround environment variables in backticks.
- reword resolvers description
- clarify how the resolvers work
- reword usage string
- remove duplicate config module
- Merge branch 'main' of https://github.com/umccr/htsget-rs into more-flexible-config-rename
- fix feature flag compile errors
- add test for long resolvers from environment variable config
- fix broken data server optional by introducing boolean flag to enable data server
- add documentation for reworked config
- flatten data server config
- clippy and fmt
- update config file with default values, add option to print a default config
- allow specifying tags, reference names, fields with an 'All' value
- remove setters, add constructors, add documentation.
- reduce some options for cors, remove repeated code when configuring cors
- fix tests affected by config, change some default values and move around config options
- remove custom deserializer for None option and instead use custom enum
- fix logic involving allowed attributes
- apply changes to other crates from reworked config
- update getter return types
- deserialize empty string as None value
- add cors tests and environment variable tests
- remove public fields, add public getters
- allow configuring multiple data servers
- add expose headers cors option
- move cors config to separate file
- add case insensitive aliases to enum variants
- add allow origins, and separate out tagged and untagged enum variants
- add generic allow type configuration option for allow headers and allow methods
- add cors max age option
- add cors allow header types for cors config
- add CorsConfig shared struct
- add UrlResolver, separate data server config from resolver
- fix errors relating to new config
- use figment instead of config because it is simpler to set defaults
- move config into separate module
- add tests for checking for contained value in interval
- implement query matcher logic
- move fields, tags, no tags, query, and interval to config
- add separate config for local server and s3 storage
- add config file from command line or env option
- swap out envy for config dependency
- Add documentation for cors
- Implement configurable origin for cors
- Add cors allow credentials option to data server config
- Move data server config to separate struct
- Move server config into separate struct
- Implement cors for htsget-http-actix.
- Add more spans and tracing calls.
- Move tracing setup to config.
- Bump dependencies, fix clippy warnings.
- Improve errors so that they are more informative.
- Update README instructions.
- Add ticker server addr test.
- Clean up code, format, update dependencies.
- Implement non-tls ticket server alongside tls ticket server.
- Fix localstorage path (#86)
- Fix tests and errors (#83)
- Deploy htsget-http-lambda. (#81)
- Enable choosing between storage types. (#80)
- Remove file from localstorage (#79)
- Htsget http lambda (#76)
