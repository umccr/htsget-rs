# Deployment of htsget-http-lambda

This is an example that deploys htsget-http-lambda using aws-cdk. The stack is deployed as a RustFunction
using [rust.aws-cdk-lambda](https://www.npmjs.com/package/rust.aws-cdk-lambda), and integrated with aws
api gateway.

To configure htsget-http-lambda change the environment variable inside the `RustFunction` props in `htsget-http-lambda-stack.ts`, 
which changes the environment variables passed to htsget-http-lambda.

## Deploying

### Prerequisites

* [aws-cli](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html) should installed and authenticated in the shell.
* Node.js and [npm](https://docs.npmjs.com/downloading-and-installing-node-js-and-npm) should be installed.
* [Rust](https://www.rust-lang.org/tools/install) should be installed.

After installing the basic dependencies, complete the following steps:

1. Add the arm cross-compilation target to rust.
3. Install [Zig](https://ziglang.org/) using one of the methods show in [getting started](https://ziglang.org/learn/getting-started/), or by running the commands below and following the prompts. Zig is used by cargo-lambda for cross-compilation.
2. Install [cargo-lambda](https://github.com/cargo-lambda/cargo-lambda), as it is used to compile artifacts that are uploaded to aws lambda.
4. Install [aws-cdk](https://docs.aws.amazon.com/cdk/v2/guide/getting_started.html) and typescript.
4. Install packages from the `deploy` directory and finally compile the lambda.

In a copy-paste nutshell, for the impatient, run the following in the `deploy` directory:

```console
npm install -g aws-cdk typescript
rustup target add aarch64-unknown-linux-gnu
cargo install cargo-lambda
npm install
cd .. && cargo lambda build --arm64
```

After this, we're ready to deploy!

### Deploy to AWS

CDK should be bootstrapped once, if this hasn't been done before. The deployment itself can be done by checking that the stack synthesizes correctly and then deploying.

```console
cdk bootstrap
cdk synth
cdk deploy
```

Towards the end of the deployment you should get an API Gateway endpoint URL, and then one can use `awscurl` to query an endpoint like so:

```
% awscurl --region ap-southeast-2 https://<RANDOM_ID>.execute-api.ap-southeast-2.amazonaws.com/prod/reads/service-info
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

Note: One can pass the `--region` accordingly to `awscurl` or use `AWS_DEFAULT_REGION` environment variable, but `AWS_REGION` is not honored.

### Local testing

The lambda function can also be run locally using cargo-lambda. From the root project directory, execute the following command.
```console
cargo lambda watch
```

Then in a **separate terminal session** run.
```console
cargo lambda invoke htsget-http-lambda --data-file data/events/event_get.json
```

Examples of different lambda events are located in the `data/events` directory.
