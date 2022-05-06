import {Duration, Stack, StackProps} from 'aws-cdk-lib';
import { Construct } from 'constructs';
import * as iam from "aws-cdk-lib/aws-iam";
import {RustFunction} from "rust.aws-cdk-lambda";
import {Architecture} from "aws-cdk-lib/aws-lambda";
import * as apigw from "aws-cdk-lib/aws-apigateway";
import {AuthorizationType} from "aws-cdk-lib/aws-apigateway";

export class HtsgetAppStack extends Stack {
  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    const lambdaRole = new iam.Role(this, id + '-role', {
      assumedBy: new iam.ServicePrincipal('lambda.amazonaws.com'),
      description: 'Lambda execution role for ' + id,
    });

    const s3BucketPolicy = new iam.PolicyStatement({
      actions: ['s3:List*', 's3:Get*'],
      resources: ['arn:aws:s3:::*'],
    });

    lambdaRole.addManagedPolicy(iam.ManagedPolicy.fromAwsManagedPolicyName("service-role/AWSLambdaBasicExecutionRole"));
    lambdaRole.addToPolicy(s3BucketPolicy);

    let htsgetLambda = new RustFunction(this, id, {
      directory: '../../htsget-http-lambda/Cargo.toml',
      memorySize: 128,
      timeout: Duration.seconds(10),
      environment: {
        HTSGET_BUCKET_NAME: 'htsget-rs-data',
        HTSGET_STORAGE_TYPE: 'AwsS3Storage'
      },
      setupLogging: true,
      architecture: Architecture.ARM_64,
      role: lambdaRole,
      target: "aarch64-unknown-linux-gnu"
    });

    new apigw.LambdaRestApi(this, id + '-api', {
      handler: htsgetLambda,
      proxy: true,
      defaultMethodOptions: {
        authorizationType: AuthorizationType.IAM,
      }
    });
  }
}
