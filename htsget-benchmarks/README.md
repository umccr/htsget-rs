# Quickstart

## Profiling

With cargo-instruments (OSX-only profiling, WIP):

```
time cargo instruments -t time --all-features --bench search-benchmarks
```

With not-perf (with criterion-rs external profiling integration, WIP):

```
cargo run record -P `cargo bench` -w -o datafile
```

## Benchmarking

```
bencher run \
  --branch main \
  --project htsget-rs \
  "cargo bench"
```
