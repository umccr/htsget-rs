import {Duration, Stack, StackProps, Tags} from 'aws-cdk-lib';
import {Construct} from 'constructs';
import * as iam from 'aws-cdk-lib/aws-iam';
import {RustFunction, Settings} from 'rust.aws-cdk-lambda';
import {Architecture} from 'aws-cdk-lib/aws-lambda';
import * as apigw from 'aws-cdk-lib/aws-apigateway';
import {AuthorizationType} from 'aws-cdk-lib/aws-apigateway';
import {STACK_NAME} from '../bin/htsget-http-lambda';

/**
 * Configuration for HtsgetHttpLambdaStack.
 */
export type Config = {
  environment: string,
  bucket: string;
  cors_allow_origins: string;
  regex: string,
  substitution_string: string;
}

/**
 * Stack used to deploy htsget-http-lambda.
 */
export class HtsgetHttpLambdaStack extends Stack {
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
    // Don't build htsget packages other than htsget-http-lambda.
    Settings.BUILD_INDIVIDUALLY = true;

    const config = this.getConfig();
    let htsgetLambda = new RustFunction(this, id + 'Function', {
      // Build htsget-http-lambda only.
      package: 'htsget-http-lambda',
      target: 'aarch64-unknown-linux-gnu',

      memorySize: 128,
      timeout: Duration.seconds(10),
      // Change environment variables passed to htsget-http-lambda.
      environment: {
        HTSGET_TICKET_SERVER_CORS_ALLOW_ORIGINS: config.cors_allow_origins,
        HTSGET_TICKET_SERVER_CORS_MAX_AGE: '300',
        HTSGET_RESOLVERS: `[{
          regex=${config.regex}, 
          substitution_string=${config.substitution_string}, 
          storage_type={type=S3, bucket=${config.bucket}}
        }]`,
        HTSGET_NAME: "umccr-htsget-rs",
        HTSGET_VERSION: "0.1.0",
        HTSGET_ORGANIZATION_NAME: "UMCCR",
        HTSGET_ORGANIZATION_URL: "https://umccr.org/",
        HTSGET_CONTACT_URL: "https://umccr.org/",
        HTSGET_DOCUMENTATION_URL: "https://github.com/umccr/htsget-rs",
        HTSGET_ENVIRONMENT: `${config.environment}`,
        RUST_LOG: 'info,htsget_http_lambda=trace,htsget_config=trace,htsget_http_core=trace,htsget_search=trace'
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

  /**
   * Get the environment configuration from cdk.json. Pass `--context "env=dev"` or `--context "env=prod"` to
   * control the environment.
   */
  getConfig(): Config {
    let env: string = this.node.tryGetContext('env');
    if (!env) {
      console.log("No environment supplied, using `dev` environment config")
      env = "dev";
    }

    const config = this.node.tryGetContext(env);
    return {
      environment: env,
      bucket: config?.bucket ?? 'umccr-primary-data-dev',
      cors_allow_origins: config?.cors_allow_origins ?? '[https://data.umccr.org, https://data.dev.umccr.org]',
      regex: config?.regex ?? '^umccr-primary-data-dev/(?P<accession>.*)$',
      substitution_string: config?.substitution_string ?? '$accession'
    };
  }
}
