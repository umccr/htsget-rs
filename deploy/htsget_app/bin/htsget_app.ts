#!/usr/bin/env node
import 'source-map-support/register';
import * as cdk from 'aws-cdk-lib';
import { HtsgetAppStack } from '../lib/htsget_app_stack';

const app = new cdk.App();
new HtsgetAppStack(app, 'HtsgetAppStack');