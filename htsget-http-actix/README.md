# HtsGet rust-based server
This crate should allow to setup an [HtsGet](http://samtools.github.io/hts-specs/htsget.html) compliant server. For that purpose it uses the htsget-search, htsget-http-core and actix-web crates as dependencies.
## Start the server
To start the server it should be enough to run `cargo run` or to execute the binary. You should keep in mind, that the server will take the path where you executed it as base path to search for files. This and other settings can be changed using environment variables:
* HTSGET_IP: The ip to use. Default: 127.0.0.1
* HTSGET_PORT: The port to use. Default: 8080
* HTSGET_PATH: The path to the directory where the server should be started. Default: Actual directory
For example, `HTSGET_PORT=8000 cargo run` will try to bind the server to the port 8000.
The next variables are used to configure the info for the service-info endpoints
* HTSGET_ID: The id of the service. Default: ""
* HTSGET_NAME: The name of the service. Default: "HtsGet service"
* HTSGET_VERSION: The version of the service. Default: ""
* HTSGET_ORGANIZATION_NAME: The name of the organization. Default: "Snake oil"
* HTSGET_ORGANIZATION_URL: The url of the organization. Default: "https://en.wikipedia.org/wiki/Snake_oil"
## Examples
These are some examples with [curl](https://github.com/curl/curl) in which the server was started at the root of the [htsget-rs project](https://github.com/umccr/htsget-rs):
* Simple GET request:
```bash
curl '127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer'
```
* Simple POST request:
```bash
curl --header "Content-Type: application/json" -d '{}' '127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer'
```
* GET request:
```bash
curl '127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer?format=VCF&class=header'
```
* POST request:
```bash
curl --header "Content-Type: application/json" -d '{"format": "VCF", "regions": [{"referenceName": "chrM"}]}' '127.0.0.1:8080/variants/data/vcf/sample1-bcbio-cancer'
```
* Service-info request:
```bash
curl 127.0.0.1:8080/variants/service-info
```