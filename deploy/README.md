# Deployment of htsget-lambda

The [htsget-lambda] crate is a cloud-based implementation of [htsget-rs]. It uses AWS Lambda as the ticket server, and AWS S3 as the data block server.

This is an example that deploys [htsget-lambda] using [aws-cdk]. It is deployed as an AWS HTTP [API Gateway Lambda proxy
integration][aws-api-gateway]. The stack uses [RustFunction][rust-function] in order to integrate [htsget-lambda]
with API Gateway. It uses a [JWT authorizer][jwt-authorizer] with [AWS Cognito][aws-cognito] as the issuer, and routes
the htsget-rs server with [AWS Route 53][route-53].

## Configuration

To configure the deployment change the config files in the [`config`][config] directory. Make sure to point to that config file on `cdk.json` before running `cdk deploy`, i.e:

```json
$ cat deploy/cdk.json
(...)
  },
  "context": {
    "htsget_rs_config": "config/public_umccr.toml",
(...)
```

These config files configure [htsget-lambda]. See [htsget-config] for a list of available configuration options.

## Deploying

### Prerequisites

1. [aws-cli] should be installed and authenticated in the shell.
1. Node.js and [npm] should be installed.
1. [Rust][rust] should be installed.
1. [Zig][zig] should be installed

After installing the basic dependencies, complete the following steps:

1. Define CDK\_DEFAULT\_* env variables (if not defined already). You must be authenticated with your AWS cloud to run this step.
1. Add the arm cross-compilation target to rust.
1. Install [cargo-lambda], as it is used to compile artifacts that are uploaded to aws lambda.
1. Define which configuration to use for htsget-rs on `cdk.json` as stated in aforementioned configuration section. 

Below is a summary of commands to run in this directory:

```sh
export CDK_DEFAULT_ACCOUNT=`aws sts get-caller-identity --query Account --output text`
export CDK_DEFAULT_REGION=`aws configure get region`
rustup target add aarch64-unknown-linux-gnu
cargo install cargo-lambda
npm install
```

### Deploy to AWS

CDK should be bootstrapped once, if this hasn't been done before.

```sh
npx cdk bootstrap
```

In order to deploy, check that the stack synthesizes correctly and then deploy.

```sh
npx cdk synth
npx cdk deploy
```

### Testing the endpoint

When the deployment is finished, the htsget endpoint can be tested by querying it. Since a JWT authorizer is used,
a valid JWT token must be obtained in order to access the endpoint. This token should be obtained from AWS Cognito using
the configured user pool id and audience parameters. Then `curl` can be used to query the endpoint:

```sh
curl -H "Authorization: <JWT Token>" "https://<htsget_domain>/reads/service-info"
```

With a possible output:

```json
{
  "id": "",
  "name": "",
  "version": "",
  "organization": {
    "name": "",
    "url": ""
  },
  "type": {
    "group": "",
    "artifact": "",
    "version": ""
  },
  "htsget": {
    "datatype": "reads",
    "formats": ["BAM", "CRAM"],
    "fieldsParametersEffective": false,
    "TagsParametersEffective": false
  },
  "contactUrl": "",
  "documentationUrl": "",
  "createdAt": "",
  "UpdatedAt": "",
  "environment": ""
}
```

[awscurl]: https://github.com/okigan/awscurl

### Local testing

The [Lambda][htsget-lambda] function can also be run locally using [cargo-lambda]. From the root project directory, execute the following command.

```sh
cargo lambda watch
```

Then in a **separate terminal session** run.

```sh
cargo lambda invoke htsget-lambda --data-file data/events/event_get.json
```

Examples of different Lambda events are located in the [`data/events`][data-events] directory.

## Docker

There are multiple options to use docker containers with htsget-rs:

### Local

```
$ docker build . -f deploy/Dockerfile -t htsget-rs-actix
$ docker run -p 8080:8080 -p 8081:8081 htsget-rs-actix
2023-10-25T01:01:38.412471Z  INFO bind_addr{addr=0.0.0.0:8081 cors=CorsConfig { allow_credentials: false, allow_origins: List([HeaderValue("http://localhost:8080")]), allow_headers: Tagged(All), allow_methods: Tagged(All), max_age: 86400, expose_headers: List([]) }}: htsget_search::storage::data_server: data server address bound to address=0.0.0.0:8081
2023-10-25T01:01:38.412710Z  INFO run_server: htsget_actix: using non-TLS ticket server
2023-10-25T01:01:38.412805Z  INFO run_server: htsget_actix: htsget query server addresses bound addresses=[0.0.0.0:8080]
2023-10-25T01:01:38.412837Z  INFO run_server: actix_server::builder: starting 8 workers
2023-10-25T01:01:38.412892Z  INFO actix_server::server: Actix runtime found; starting in Actix runtime
```

### Local with LocalStack (local AWS)

```
$ cd deploy
$ docker compose up --wait -d
$ npx cdklocal bootstrap
$ npx cdklocal deploy
```

### Local with MinIO (S3) backend

TBD, fetch instructions from [NBIS Sweden usecase, test and document them here properly](https://github.com/NBISweden/htsget-rs/tree/docker-testing/deploy).

[htsget-lambda]: ../htsget-lambda
[cargo-lambda]: https://github.com/cargo-lambda/cargo-lambda
[data-events]: ../data/events
[htsget-rs]: https://github.com/umccr/htsget-rs
[htsget-lambda]: ../htsget-lambda
[htsget-config]: ../htsget-config
[config]: config
[aws-cdk]: https://docs.aws.amazon.com/cdk/v2/guide/getting_started.html
[cdk-context]: https://docs.aws.amazon.com/cdk/v2/guide/context.html
[cdk-lookup-value]: https://docs.aws.amazon.com/cdk/api/v2/docs/aws-cdk-lib.aws_ssm.StringParameter.html#static-valuewbrfromwbrlookupscope-parametername
[cdk-json]: cdk.json
[aws-ssm]: https://docs.aws.amazon.com/systems-manager/latest/userguide/systems-manager-parameter-store.html
[aws-api-gateway]: https://docs.aws.amazon.com/apigateway/latest/developerguide/http-api-develop-integrations-lambda.html
[aws-cognito]: https://docs.aws.amazon.com/cognito/latest/developerguide/cognito-user-identity-pools.html
[jwt-authorizer]: https://docs.aws.amazon.com/apigateway/latest/developerguide/http-api-jwt-authorizer.html
[jwt-audience]: https://docs.aws.amazon.com/apigatewayv2/latest/api-reference/apis-apiid-authorizers-authorizerid.html#apis-apiid-authorizers-authorizerid-model-jwtconfiguration
[route-53]: https://docs.aws.amazon.com/Route53/latest/DeveloperGuide/Welcome.html
[rust-function]: https://www.npmjs.com/package/rust.aws-cdk-lambda
[aws-cdk]: https://docs.aws.amazon.com/cdk/v2/guide/getting_started.html
[aws-cli]: https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html
[npm]: https://docs.npmjs.com/downloading-and-installing-node-js-and-npm
[rust]: https://www.rust-lang.org/tools/install
[zig]: https://ziglang.org/
[zig-getting-started]: https://ziglang.org/learn/getting-started/
