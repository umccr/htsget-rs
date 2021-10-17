# AWS Rust lambda http backend for htsget

This crate provides the endpoints needed for htsget, that is `reads`, `variants` and its accompanying `service-info` routes. The current implementation only goes for an `async` since no blocking exists for lambda_rust_runtime at this point.

## Quickstart

### TODO: Indicate if the deployment is on top level of whether the user has to come to this dir (the latter seems most reasonable).
To deploy this on AWS proper run with AWS SAM CLI:

```
$ sam build
$ sam deploy
```

If you are facing difficulties in Apple silicon computers, the fix is a bit more involved, see: https://github.com/umccr/s3-rust-noodles-bam#quickstart