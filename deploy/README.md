# Deployment of htsget-http-lambda

The [htsget-http-lambda] crate is a cloud-based implementation of [htsget-rs]. It uses AWS Lambda as the ticket server, and AWS S3 as the data block server. 

This is an example that deploys [htsget-http-lambda] using [aws-cdk]. It is deployed as an AWS Rest [API Gateway Lambda proxy 
integration][aws-api-gateway]. The stack uses [RustFunction][rust-function] in order to integrate [htsget-http-lambda]
with API Gateway.

To configure the deployment change the environment variable inside the `RustFunction` props in 
[`htsget-http-lambda-stack.ts`][htsget-http-lambda-stack]. This changes the environment variables passed to [htsget-http-lambda].

See [htsget-config] for a list of available configuration options.

[htsget-rs]: https://github.com/umccr/htsget-rs
[htsget-http-lambda]: ../htsget-http-lambda
[htsget-config]: ../htsget-config
[htsget-http-lambda-stack]: lib/htsget-http-lambda-stack.ts
[aws-cdk]: https://docs.aws.amazon.com/cdk/v2/guide/getting_started.html
[aws-api-gateway]: https://docs.aws.amazon.com/apigateway/latest/developerguide/set-up-lambda-proxy-integrations.html
[rust-function]: https://www.npmjs.com/package/rust.aws-cdk-lambda

## Deploying

### Prerequisites

* [aws-cli] should installed and authenticated in the shell.
* Node.js and [npm] should be installed.
* [Rust][rust] should be installed.

After installing the basic dependencies, complete the following steps:

1. Add the arm cross-compilation target to rust.
3. Install [Zig][zig] using one of the methods show in [getting started][zig-getting-started], or by running the commands below and following the prompts. Zig is used by cargo-lambda for cross-compilation.
2. Install [cargo-lambda], as it is used to compile artifacts that are uploaded to aws lambda.
4. Install [aws-cdk] and typescript.
4. Install packages from this directory and compile [htsget-http-lambda]. This should place artifacts compiled for arm64 under the `target/lambda` directory which can be deployed to AWS.

Below is a summary of commands to run in this directory:

```shell
npm install -g aws-cdk typescript
rustup target add aarch64-unknown-linux-gnu
cargo install cargo-lambda
npm install

cd ..
cargo lambda build --release --arm64 --bin htsget-http-lambda
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

```shell
cdk bootstrap
```

In order to deploy, check that the 
stack synthesizes correctly and then deploy.

```shell
cdk synth
cdk deploy
```

Towards the end of the deployment you should get an API Gateway endpoint. This can be used with [awscurl] to query the htsget-rs server:

```shell
awscurl --region ap-southeast-2 https://<ID>.execute-api.ap-southeast-2.amazonaws.com/prod/reads/service-info
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
    "formats": [
      "BAM",
      "CRAM"
    ],
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

It's recommended to use the `--region` flag with `awscurl` as environment variables may not work.

[awscurl]: https://github.com/okigan/awscurl

### Local testing

The [Lambda][htsget-http-lambda] function can also be run locally using [cargo-lambda]. From the root project directory, execute the following command.
```console
cargo lambda watch
```

Then in a **separate terminal session** run.
```console
cargo lambda invoke htsget-http-lambda --data-file data/events/event_get.json
```

Examples of different Lambda events are located in the [`data/events`][data-events] directory.

[htsget-http-lambda]: ../htsget-http-lambda
[cargo-lambda]: https://github.com/cargo-lambda/cargo-lambda
[data-events]: ../data/events