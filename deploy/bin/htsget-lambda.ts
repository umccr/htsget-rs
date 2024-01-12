#!/usr/bin/env node
import "source-map-support/register";
import * as cdk from "aws-cdk-lib";
import { HtsgetLambdaStack } from "../lib/htsget-lambda-stack";

export const STACK_NAME = "HtsgetLambdaStackPublic";
const STACK_DESCRIPTION = "A stack deploying htsget-lambda with API gateway.";

const app = new cdk.App();
new HtsgetLambdaStack(
  app,
  STACK_NAME,
  {
    stackName: STACK_NAME,
    description: STACK_DESCRIPTION,
    tags: {
      Stack: STACK_NAME,
    },
    env: {
      account: process.env.CDK_DEFAULT_ACCOUNT,
      region: process.env.CDK_DEFAULT_REGION,
    },
  },
  {
    config: "config/public_umccr.toml",
    domain: "htsget.demo.umccr.org",
    s3BucketResources: [
      "arn:aws:s3:::org.umccr.demo.sbeacon-data/*",
      "arn:aws:s3:::org.umccr.demo.htsget-rs-data/*",
    ],
  },
);
