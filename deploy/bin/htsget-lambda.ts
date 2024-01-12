#!/usr/bin/env node
import "source-map-support/register";
import * as cdk from "aws-cdk-lib";
import { HtsgetLambdaStack } from "../lib/htsget-lambda-stack";
import { SETTINGS } from "./settings";

export const STACK_NAME = "HtsgetLambdaStack";
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
  SETTINGS,
);
