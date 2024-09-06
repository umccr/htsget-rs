# MinIO deployment

[MinIO][minio] can be used with htsget-rs by configuring the [storage type][storage] as `S3` and setting the `endpoint` to the MinIO server.
There are a few specific configuration options that need to be considered to use MinIO with htsget-rs, and those include:

* The standard [AWS environment variables][env-variables] for connecting to AWS services must be set, and configured to match those
used by MinIO.
    * This means that htsget-rs expects an `AWS_DEFAULT_REGION` to be set, which must match the region used by MinIO (by default us-east-1).
    * It also means that the `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` must be set to match the credentials used by MinIO.
* If using virtual-hosted style [addressing][virtual-addressing] instead of path style [addressing][path-addressing], `MINIO_DOMAIN` must be
set on the MinIO server and DNS resolution must allow accessing the MinIO server using `bucket.<MINIO_DOMAIN>`.
    * Path style addressing can be used instead by setting `path_style = true` under the htsget-rs resolvers storage type.

The caveats around the addressing style occur because there are two different addressing styles for S3 buckets, path style, e.g.
`http://minio:9000/bucket`, and virtual-hosted style, e.g. `http://bucket.minio:9000`. AWS has declared path style addressing
as [deprecated][path-style-deprecated], so this example sets up virtual-hosted style addressing as the default.

## Deployment using Docker

The above configuration can be applied using docker-compose to set the relevant environment variables. Additionally, if using
docker compose and virtual-hosted style addressing, a network alias which allows accessing the MinIO service under `bucket.<MINIO_DOMAIN>`
must be present.

An example [`compose.yml`][compose] is available which shows htsget-rs configured to use MinIO, serving data from the [data] directory.

After running the compose file, requests can be fetched using htsget:

```sh
docker compose up
```

Then:

```sh
curl http://127.0.0.1:8080/reads/bam/htsnexus_test_NA12878
```

Outputs:

```sh
{
  "htsget": {
    "format": "BAM",
    "urls": [
      {
        "url": "http://data.minio:9000/bam/htsnexus_test_NA12878.bam?x-id=GetObject&X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=user%2F20240320%2Fus-east-1%2Fs3%2Faws4_request&X-Amz-Date=20240320T014007Z&X-Amz-Expires=1000&X-Amz-SignedHeaders=host%3Brange&X-Amz-Signature=33a75bd6363ccbfd5ce8edf7e102a5edff8ca7cee17e3c654db01a880e98072d",
        "headers": {
          "Range": "bytes=0-2596770"
        }
      },
      {
        "url": "data:;base64,H4sIBAAAAAAA/wYAQkMCABsAAwAAAAAAAAAAAA=="
      }
    ]
  }
}
```

The url tickets can then be fetched within the compose network context:

```sh
docker exec -it minio curl -H "Range: bytes=0-2596770" "http://data.minio:9000/bam/htsnexus_test_NA12878.bam?x-id=GetObject&X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=user%2F20240320%2Fus-east-1%2Fs3%2Faws4_request&X-Amz-Date=20240320T014007Z&X-Amz-Expires=1000&X-Amz-SignedHeaders=host%3Brange&X-Amz-Signature=33a75bd6363ccbfd5ce8edf7e102a5edff8ca7cee17e3c654db01a880e98072d"
```

[path-style-deprecated]: https://aws.amazon.com/blogs/aws/amazon-s3-path-deprecation-plan-the-rest-of-the-story/
[storage]: ../../../htsget-config/README.md#resolvers
[minio]: https://min.io/
[env-variables]: https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-envvars.html
[virtual-addressing]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/VirtualHosting.html#virtual-hosted-style-access
[path-addressing]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/VirtualHosting.html#path-style-access
[compose]: compose.yml
[data]: ../../../data
