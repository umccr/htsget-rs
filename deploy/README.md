# Deployment of htsget-http-lambda

This is an example that deploys htsget-http-lambda using aws-cdk. The stack is deployed as a RustFunction
using [rust.aws-cdk-lambda](https://www.npmjs.com/package/rust.aws-cdk-lambda), and integrated with aws
api gateway.

To configure htsget-http-lambda change the environment variable inside the `RustFunction` props in `htsget-http-lambda-stack.ts`, 
which changes the environment variables passed to htsget-http-lambda.

## Deploying

### Prerequisites

* [aws-cli](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html) should installed and authenticated in the shell by running `aws configure`.
* Node.js and [npm](https://docs.npmjs.com/downloading-and-installing-node-js-and-npm) should be installed.
* [Rust](https://www.rust-lang.org/tools/install) should be installed.

After installing the basic dependencies, the following should also be completed.

* Add the arm cross-compilation target to rust.
```console
rustup target add aarch64-unknown-linux-gnu
```
* Install [cargo-lambda](https://github.com/cargo-lambda/cargo-lambda), as it is used to compile artifacts that are uploaded to aws lambda.
```console
cargo install cargo-lambda
```
* Install [Zig](https://ziglang.org/) using one of the methods show in [getting started](https://ziglang.org/learn/getting-started/), or by running the command below in the root project directory and following the prompts. Zig is used by cargo-lambda for cross-compilation.
```console
cargo lambda build --arm64
```
* Install [aws-cdk](https://docs.aws.amazon.com/cdk/v2/guide/getting_started.html) and typescript.
```console
npm install -g aws-cdk typescript
```
* Install packages from the `deploy` directory.
```console
npm install
```

### Deploy
Cdk should be bootstrapped once, if this hasn't been done before.
```console
cdk bootstrap
```
Deployment can be done by checking the stack synthesizes and then deploying.
```console
cdk synth
cdk deploy
```

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
