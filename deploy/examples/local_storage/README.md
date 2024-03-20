# LocalStorage deployment

A simple [`LocalStorage`][local] deployment using default settings is available under the [`compose.yml`][compose] file in this directory.

To run, use:

```
docker compose up
```

This launches a `LocalStorage` htsget-actix server serving data from the [`data`][data] directory.

[local]: ../../../htsget-config/README.md#resolvers
[compose]: compose.yml
[data]: ../../../data