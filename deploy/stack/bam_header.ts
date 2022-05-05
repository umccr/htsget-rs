#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib';
import {ServerlessBioinformaticsStack} from './serverless_bioinformatics_stack';

const app = new cdk.App();
new ServerlessBioinformaticsStack(app, 's3-rust-noodles-bam');
