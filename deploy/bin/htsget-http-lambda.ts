#!/usr/bin/env node
import 'source-map-support/register';
import * as cdk from 'aws-cdk-lib';
import {HtsgetHttpLambdaStack} from '../lib/htsget-http-lambda-stack';

export const STACK_NAME = 'HtsgetHttpLambdaStack';
const STACK_DESCRIPTION = 'An example stack for testing htsget-http-lambda with API gateway.';

const app = new cdk.App();
new HtsgetHttpLambdaStack(app, STACK_NAME, {
    stackName: STACK_NAME,
    description: STACK_DESCRIPTION,
    tags: {
        Stack: STACK_NAME,
    },
});