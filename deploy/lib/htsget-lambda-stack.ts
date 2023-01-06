import {Duration, Stack, StackProps, Tags} from 'aws-cdk-lib';
import {Construct} from 'constructs';
import * as iam from 'aws-cdk-lib/aws-iam';
import {RustFunction, Settings} from 'rust.aws-cdk-lambda';
import {Architecture} from 'aws-cdk-lib/aws-lambda';
import * as apigw from 'aws-cdk-lib/aws-apigateway';
import {AuthorizationType} from 'aws-cdk-lib/aws-apigateway';
import {STACK_NAME} from '../bin/htsget-lambda';

export class HtsgetLambdaStack extends Stack {
  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    Tags.of(this).add("Stack", STACK_NAME);

    const lambdaRole = new iam.Role(this, id + 'Role', {
      assumedBy: new iam.ServicePrincipal('lambda.amazonaws.com'),
      description: 'Lambda execution role for ' + id,
    });

    const s3BucketPolicy = new iam.PolicyStatement({
      actions: ['s3:List*', 's3:Get*'],
      resources: ['arn:aws:s3:::*'],
    });

    lambdaRole.addManagedPolicy(iam.ManagedPolicy.fromAwsManagedPolicyName('service-role/AWSLambdaBasicExecutionRole'));
    lambdaRole.addToPolicy(s3BucketPolicy);

    // Set the workspace directory of htsget.
    Settings.WORKSPACE_DIR = '../';
    // Don't build htsget packages other than htsget-lambda.
    Settings.BUILD_INDIVIDUALLY = true;

    let htsgetLambda = new RustFunction(this, id + 'Function', {
      // Build htsget-lambda only.
      package: 'htsget-lambda',
      target: 'aarch64-unknown-linux-gnu',

      memorySize: 128,
      timeout: Duration.seconds(10),
      // Change environment variables passed to htsget-lambda.
      environment: {
        HTSGET_S3_BUCKET: 'umccr-research-dev',
        HTSGET_STORAGE_TYPE: 'AwsS3Storage',
        RUST_LOG: 'info,htsget_lambda=trace,htsget_config=trace,htsget_http=trace,htsget_search=trace'
      },
      architecture: Architecture.ARM_64,
      role: lambdaRole
    });

    new apigw.LambdaRestApi(this, id + 'ApiGw', {
      handler: htsgetLambda,
      proxy: true,
      defaultMethodOptions: {
        authorizationType: AuthorizationType.IAM,
      }
    });
  }
}
