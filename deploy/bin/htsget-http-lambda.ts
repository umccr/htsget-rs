#!/usr/bin/env node
import 'source-map-support/register';
import * as cdk from 'aws-cdk-lib';
import { HtsgetHttpLambdaStack } from '../lib/htsget-http-lambda-stack';

const app = new cdk.App();
new HtsgetHttpLambdaStack(app, 'HtsgetHttpLambdaStack');