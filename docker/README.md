# Docker for htsget-rs

This directory contains a Dockerfile for htsget-rs, which is [published] to the GitHub container registry.

The Dockerfile can be build by running the following from the root of the repository:

```sh
docker build -t htsget-rs -f docker/Dockerfile .
```

See [examples] for usages with local and [MinIO][minio] config.

[published]: https://github.com/umccr/htsget-rs/pkgs/container/htsget-rs
[examples]: examples
[minio]: https://min.io/