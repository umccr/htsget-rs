# Deployment of htsget-lambda

The [htsget-lambda] crate is a cloud-based implementation of [htsget-rs]. It uses AWS Lambda as the ticket server, and AWS S3 as the data block server.

This is an example that deploys [htsget-lambda] using [aws-cdk]. It is deployed as an AWS HTTP [API Gateway Lambda proxy
integration][aws-api-gateway]. The stack uses [RustFunction][rust-function] in order to integrate [htsget-lambda]
with API Gateway. It uses a [JWT authorizer][jwt-authorizer] with [AWS Cognito][aws-cognito] as the issuer, and routes
the htsget-rs server with [AWS Route 53][route-53].

## Configuration

To configure the deployment change the config files in the [`config`][config] directory. There are two configuration files
corresponding to the deployed environment. The config file used for deployment can be controlled by passing `--context "env=dev"` or
`--context "env=prod"` to cdk. When no context parameter is supplied, the default context is `dev`.

These config files configure [htsget-lambda]. See [htsget-config] for a list of available configuration options.

There is additional config that modifies the AWS infrastructure surrounding [htsget-lambda]. This config is sourced
from [AWS System Manager Parameter Store][aws-ssm]. Parameters are fetched using [`StringParemeter.valueFromLookup`][cdk-lookup-value]
which takes a parameter name. The properties in [`cdk.json`][cdk-json] `parameter_store_names` hold these parameter names,
which correspond to the following values:

| Property storing SSM parameter name | Description of SSM parameter                           |
| ----------------------------------- | ------------------------------------------------------ |
| `arn_cert`                          | The ARN for the ACM certificate of the htsget domain.  |
| `jwt_aud`                           | The JWT [audience][jwt-audience] for the token.        |
| `cog_user_pool_id`                  | The Cognito user pool id which controls authorization. |
| `htsget_domain`                     | The domain name for the htsget server.                 |
| `hosted_zone_id`                    | The Route 53 hosted zone id for the htsget server.     |
| `hosted_zone_name`                  | The Route 53 hosted zone name for the htsget server.   |

Modify these properties to change which parameter the values come from. SSM parameters can also be cached locally using
the [CDK runtime context][cdk-context].

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

## Deploying

### Prerequisites

- [aws-cli] should be installed and authenticated in the shell.
- Node.js and [npm] should be installed.
- [Rust][rust] should be installed.

After installing the basic dependencies, complete the following steps:

1. Add the arm cross-compilation target to rust.
2. Install [Zig][zig] using one of the methods show in [getting started][zig-getting-started], or by running the commands below and following the prompts. Zig is used by cargo-lambda for cross-compilation.
3. Install [cargo-lambda], as it is used to compile artifacts that are uploaded to aws lambda.
4. Install packages from this directory and compile [htsget-lambda]. This should place artifacts compiled for arm64 under the `target/lambda` directory which can be deployed to AWS.

Below is a summary of commands to run in this directory:

```sh
rustup target add aarch64-unknown-linux-gnu
cargo install cargo-lambda
npm install

cd ..
cargo lambda build --release --arm64 --bin htsget-lambda --features s3-storage
cd deploy
```

[aws-cdk]: https://docs.aws.amazon.com/cdk/v2/guide/getting_started.html
[aws-cli]: https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html
[npm]: https://docs.npmjs.com/downloading-and-installing-node-js-and-npm
[rust]: https://www.rust-lang.org/tools/install
[zig]: https://ziglang.org/
[zig-getting-started]: https://ziglang.org/learn/getting-started/

### Deploy to AWS

CDK will run many of the commands above again. However, it is recommended to run them once before trying the commands below,
to ensure that prerequisites are met.

CDK should be bootstrapped once, if this hasn't been done before.

```sh
npx cdk bootstrap
```

In order to deploy, check that the
stack synthesizes correctly and then deploy.

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

### Local with MinIO (S3) backend

TBD

[htsget-lambda]: ../htsget-lambda
[cargo-lambda]: https://github.com/cargo-lambda/cargo-lambda
[data-events]: ../data/events
