import {Duration, Stack, StackProps, Tags} from 'aws-cdk-lib';
import {Construct} from 'constructs';
import * as iam from 'aws-cdk-lib/aws-iam';
import {RustFunction, Settings} from 'rust.aws-cdk-lambda';
import {Architecture} from 'aws-cdk-lib/aws-lambda';
import * as apigwv2 from '@aws-cdk/aws-apigatewayv2-alpha';
import {STACK_NAME} from '../bin/htsget-lambda';
import {HttpLambdaIntegration} from "@aws-cdk/aws-apigatewayv2-integrations-alpha";
import {StringParameter} from "aws-cdk-lib/aws-ssm";
import {HttpJwtAuthorizer} from "@aws-cdk/aws-apigatewayv2-authorizers-alpha";
import {ARecord, HostedZone, RecordTarget} from "aws-cdk-lib/aws-route53";
import {ApiGatewayv2DomainProperties} from "aws-cdk-lib/aws-route53-targets";
import {Certificate} from "aws-cdk-lib/aws-certificatemanager";

/**
 * Configuration for htsget-rs resolvers.
 */
export type Resolvers = {
  regex: string;
  substitution_string: string;
  storage_type: {type: string, bucket: string};
}

/**
 * Configuration for HtsgetLambdaStack.
 */
export type Config = {
  environment: string,
  resolvers: Resolvers[];
  cors_allow_origins: string[];
}

/**
 * Configuration values obtained from SSM.
 */
export type SSMConfig = {
  cert_apse2_arn: string;
  hosted_zone_id: string;
  hosted_zone_name: string;
  domain_name: string;
  cog_user_pool_id: string;
  cog_app_client_id_stage: string;
  cog_app_client_id_local: string;
}

/**
 * Stack used to deploy htsget-lambda.
 */
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

    const config = this.getConfig();
    let htsgetLambda = new RustFunction(this, id + 'Function', {
      // Build htsget-lambda only.
      package: 'htsget-lambda',
      target: 'aarch64-unknown-linux-gnu',

      memorySize: 128,
      timeout: Duration.seconds(10),
      // Change environment variables passed to htsget-lambda.
      environment: {
        HTSGET_TICKET_SERVER_CORS_ALLOW_ORIGINS: `[${config.cors_allow_origins.toString()}]`,
        HTSGET_TICKET_SERVER_CORS_MAX_AGE: '300',
        HTSGET_RESOLVERS: HtsgetLambdaStack.configToEnv(config.resolvers),
        HTSGET_NAME: "umccr-htsget-rs",
        HTSGET_VERSION: "\"0.1\"",
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

    const ssmConfig = this.getSSMConfig();
    const httpIntegration = new HttpLambdaIntegration(id + 'HtsgetIntegration', htsgetLambda);
    const authorizer = new HttpJwtAuthorizer(id + "HtsgetAuthorizer", `https://cognito-idp.${this.region}.amazonaws.com/${ssmConfig.cog_user_pool_id}`, {
      identitySource: ['$request.header.Authorization'],
      jwtAudience: [
        ssmConfig.cog_app_client_id_stage,
        ssmConfig.cog_app_client_id_local,
      ]
    });

    const domainName = new apigwv2.DomainName(
        this,
        id + "HtsgetDomainName",
        {
          certificate: Certificate.fromCertificateArn(this, id + 'HtsgetDomainCert', ssmConfig.cert_apse2_arn),
          domainName: ssmConfig.domain_name
        }
    );
    const hostedZone = HostedZone.fromHostedZoneAttributes(
        this,
        id + 'HtsgetHostedZone',
        {
          hostedZoneId: ssmConfig.hosted_zone_id,
          zoneName: ssmConfig.hosted_zone_name
        }
    );
    new ARecord(
        this,
        id + 'HtsgetARecord',
        {
          zone: hostedZone,
          recordName: 'htsget',
          target: RecordTarget.fromAlias(
              new ApiGatewayv2DomainProperties(domainName.regionalDomainName, domainName.regionalHostedZoneId)
          )
        }
    );

    new apigwv2.HttpApi(this, id + 'ApiGw', {
      defaultIntegration: httpIntegration,
      defaultAuthorizer: authorizer,
      defaultDomainMapping: {
          domainName: domainName
      },
      corsPreflight: {
        allowOrigins: config.cors_allow_origins,
        allowHeaders: ['*'],
        allowMethods: [apigwv2.CorsHttpMethod.ANY],
        allowCredentials: true
      }
    });
  }

  /**
   * Get config values from SSM.
   */
  getSSMConfig(): SSMConfig {
    return {
      cert_apse2_arn: StringParameter.fromStringParameterName(this, 'SSLCertAPSE2ARN', '/htsget/acm/apse2_arn').stringValue,
      cog_app_client_id_local: StringParameter.fromStringParameterName(this, 'CogAppClientIDLocal', '/data_portal/client/cog_app_client_id_local').stringValue,
      cog_app_client_id_stage: StringParameter.fromStringParameterName(this, 'CogAppClientIDStage', '/data_portal/client/cog_app_client_id_stage').stringValue,
      cog_user_pool_id: StringParameter.fromStringParameterName(this, 'CogUserPoolID', '/data_portal/client/cog_user_pool_id').stringValue,
      domain_name: StringParameter.fromStringParameterName(this, 'DomainName', '/htsget/domain').stringValue,
      hosted_zone_id: StringParameter.fromStringParameterName(this, 'HostedZoneID', 'hosted_zone_id').stringValue,
      hosted_zone_name: StringParameter.fromStringParameterName(this, 'HostedZoneName', 'hosted_zone_name').stringValue,
    }
  }

  /**
   * Convert JSON config to htsget-rs env representation.
   */
  static configToEnv(config: any): string {
    return JSON.stringify(config).replaceAll(":", "=");
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
      resolvers: config?.resolvers ?? [{
        storage_type: { type: "S3", bucket: "umccr-primary-data-dev" },
        regex: '^umccr-primary-data-dev/(?P<key>.*)$',
        substitution_string: '$key'
      }],
      cors_allow_origins: config?.cors_allow_origins ?? '[https://data.umccr.org, https://data.dev.umccr.org]',
    };
  }
}
