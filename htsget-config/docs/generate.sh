#!/usr/bin/env bash

set -euxo pipefail

# Update the default config toml and authorization response schema.
cargo run -p htsget-axum --all-features -- -p > examples/default.toml
cargo run -p htsget-axum --all-features -- -s > schemas/auth.schema.json
pre-commit run --files schemas/auth.schema.json examples/default.toml || true
