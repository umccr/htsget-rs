# File-based deployment

A [`File`][local] deployment using default settings is available under the [`compose.yml`][compose] file in this directory.

To run, use:

```
docker compose up
```

This launches a `File` htsget-actix server serving data from the [`data`][data] directory.

The htsget-rs server can then be queried:

```sh
curl http://127.0.0.1:8080/reads/data/bam/seraseq_cebpa_larger
```

Which outputs:
```sh
{
  "htsget": {
    "format": "BAM",
    "urls": [
      {
        "url": "http://0.0.0.0:8081/data/bam/seraseq_cebpa_larger.bam",
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

The volumes of the [`compose.yml`][compose] can be changed to any directory to serve data from it using
default settings, and `curl http://127.0.0.1:8080/reads/data/<id>`, noting the extra `data` prefix.

[local]: ../../../htsget-config/README.md
[compose]: compose.yml
[data]: ../../../data
