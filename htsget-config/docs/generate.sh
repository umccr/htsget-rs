#!/usr/bin/env bash

set -euxo pipefail

# Update the default config toml and authorization response schema.
cargo run -p htsget-axum -- -p > examples/default.toml
cargo run -p htsget-axum -- -s > schemas/auth.schema.json
