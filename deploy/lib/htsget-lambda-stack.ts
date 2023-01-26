import { Duration, Stack, StackProps, Tags } from 'aws-cdk-lib';
import { Construct } from 'constructs';
import * as iam from 'aws-cdk-lib/aws-iam';
import { RustFunction, Settings } from 'rust.aws-cdk-lambda';
import { Architecture } from 'aws-cdk-lib/aws-lambda';
import * as apigwv2 from '@aws-cdk/aws-apigatewayv2-alpha';
import { STACK_NAME } from '../bin/htsget-lambda';
import { HttpLambdaIntegration } from '@aws-cdk/aws-apigatewayv2-integrations-alpha';
import { StringParameter } from 'aws-cdk-lib/aws-ssm';
import { HttpJwtAuthorizer } from '@aws-cdk/aws-apigatewayv2-authorizers-alpha';
import { ARecord, HostedZone, RecordTarget } from 'aws-cdk-lib/aws-route53';
import { ApiGatewayv2DomainProperties } from 'aws-cdk-lib/aws-route53-targets';
import { Certificate } from 'aws-cdk-lib/aws-certificatemanager';
import * as fs from 'fs';
import * as TOML from '@iarna/toml';

/**
 * Configuration for HtsgetLambdaStack.
 */
export type Config = {
  environment: string;
  htsgetConfig: { [key: string]: any };
  allowCredentials?: boolean;
  allowHeaders?: string[];
  allowMethods?: apigwv2.CorsHttpMethod[];
  allowOrigins?: string[];
  exposeHeaders?: string[];
  maxAge?: Duration;
  parameterStoreConfig: ParameterStoreConfig;
};

/**
 * Configuration values obtained from AWS System Manager Parameter Store.
 */
export type ParameterStoreConfig = {
  arnCert: string;
  hostedZoneId: string;
  hostedZoneName: string;
  htsgetDomain: string;
  cogUserPoolId: string;
  jwtAud: string[];
};

/**
 * Stack used to deploy htsget-lambda.
 */
export class HtsgetLambdaStack extends Stack {
  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    Tags.of(this).add('Stack', STACK_NAME);

    const lambdaRole = new iam.Role(this, id + 'Role', {
      assumedBy: new iam.ServicePrincipal('lambda.amazonaws.com'),
      description: 'Lambda execution role for ' + id,
    });

    const s3BucketPolicy = new iam.PolicyStatement({
      actions: ['s3:List*', 's3:Get*'],
      resources: ['arn:aws:s3:::*'],
    });

    lambdaRole.addManagedPolicy(
      iam.ManagedPolicy.fromAwsManagedPolicyName(
        'service-role/AWSLambdaBasicExecutionRole'
      )
    );
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
      environment: {
        ...config.htsgetConfig,
        RUST_LOG:
          'info,htsget_http_lambda=trace,htsget_config=trace,htsget_http_core=trace,htsget_search=trace',
      },
      architecture: Architecture.ARM_64,
      role: lambdaRole,
    });

    const parameterStoreConfig = config.parameterStoreConfig;
    const httpIntegration = new HttpLambdaIntegration(
      id + 'HtsgetIntegration',
      htsgetLambda
    );
    const authorizer = new HttpJwtAuthorizer(
      id + 'HtsgetAuthorizer',
      `https://cognito-idp.${this.region}.amazonaws.com/${parameterStoreConfig.cogUserPoolId}`,
      {
        identitySource: ['$request.header.Authorization'],
        jwtAudience: parameterStoreConfig.jwtAud,
      }
    );

    const domainName = new apigwv2.DomainName(this, id + 'HtsgetDomainName', {
      certificate: Certificate.fromCertificateArn(
        this,
        id + 'HtsgetDomainCert',
        parameterStoreConfig.arnCert
      ),
      domainName: parameterStoreConfig.htsgetDomain,
    });
    const hostedZone = HostedZone.fromHostedZoneAttributes(
      this,
      id + 'HtsgetHostedZone',
      {
        hostedZoneId: parameterStoreConfig.hostedZoneId,
        zoneName: parameterStoreConfig.hostedZoneName,
      }
    );
    new ARecord(this, id + 'HtsgetARecord', {
      zone: hostedZone,
      recordName: 'htsget',
      target: RecordTarget.fromAlias(
        new ApiGatewayv2DomainProperties(
          domainName.regionalDomainName,
          domainName.regionalHostedZoneId
        )
      ),
    });

    new apigwv2.HttpApi(this, id + 'ApiGw', {
      defaultIntegration: httpIntegration,
      defaultAuthorizer: authorizer,
      defaultDomainMapping: {
        domainName: domainName,
      },
      corsPreflight: {
        allowCredentials: config.allowCredentials,
        allowHeaders: config.allowHeaders,
        allowMethods: config.allowMethods,
        allowOrigins: config.allowOrigins,
        exposeHeaders: config.exposeHeaders,
        maxAge: config.maxAge,
      },
    });
  }

  /**
   * Get config values from the Parameter Store.
   */
  getParameterStoreConfig(config: any): ParameterStoreConfig {
    const parameterStoreNames = config.parameter_store_names;
    return {
      arnCert: StringParameter.valueFromLookup(
        this,
        parameterStoreNames.arn_cert
      ),
      jwtAud: parameterStoreNames.jwt_aud.map((jwtAud: string) =>
        StringParameter.valueFromLookup(this, jwtAud)
      ),
      cogUserPoolId: StringParameter.valueFromLookup(
        this,
        parameterStoreNames.cog_user_pool_id
      ),
      htsgetDomain: StringParameter.valueFromLookup(
        this,
        parameterStoreNames.htsget_domain
      ),
      hostedZoneId: StringParameter.valueFromLookup(
        this,
        parameterStoreNames.hosted_zone_id
      ),
      hostedZoneName: StringParameter.valueFromLookup(
        this,
        parameterStoreNames.hosted_zone_name
      ),
    };
  }

  /**
   * Convert JSON config to htsget-rs env representation.
   */
  static configToEnv(config: any): { [key: string]: string } {
    const out: { [key: string]: string } = {};
    for (const key in config) {
      out[`HTSGET_${key.toUpperCase()}`] = TOML.stringify.value(config[key]);
    }
    return out;
  }

  /**
   * Convert htsget-rs CORS option to CORS options for API Gateway.
   */
  static convertCors(configToml: any, corsValue: string): string[] | undefined {
    const value = configToml[corsValue];

    if (
      value !== undefined &&
      (value.toString().toLowerCase() === 'all' ||
        value.toString().toLowerCase() === 'mirror')
    ) {
      return ['*'];
    } else if (Array.isArray(value)) {
      return value;
    }

    return undefined;
  }

  /**
   * Convert a string CORS allowMethod option to CorsHttpMethod.
   */
  static corsAllowMethodToHttpMethod(
    corsAllowMethod?: string[]
  ): apigwv2.CorsHttpMethod[] | undefined {
    if (corsAllowMethod?.length === 1 && corsAllowMethod.includes('*')) {
      return [apigwv2.CorsHttpMethod.ANY];
    } else {
      return corsAllowMethod?.map(
        (element) =>
          apigwv2.CorsHttpMethod[element as keyof typeof apigwv2.CorsHttpMethod]
      );
    }
  }

  /**
   * Get the environment configuration from cdk.json. Pass `--context "env=dev"` or `--context "env=prod"` to
   * control the environment.
   */
  getConfig(): Config {
    let env: string = this.node.tryGetContext('env');
    if (!env) {
      console.log('No environment supplied, using `dev` environment config');
      env = 'dev';
    }

    const config = this.node.tryGetContext(env);
    const configToml = TOML.parse(fs.readFileSync(config.config).toString());

    return {
      environment: env,
      htsgetConfig: HtsgetLambdaStack.configToEnv(configToml),
      allowCredentials:
        configToml.ticket_server_cors_allow_credentials as boolean,
      allowHeaders: HtsgetLambdaStack.convertCors(
        configToml,
        'ticket_server_cors_allow_headers'
      ),
      allowMethods: HtsgetLambdaStack.corsAllowMethodToHttpMethod(
        HtsgetLambdaStack.convertCors(
          configToml,
          'ticket_server_cors_allow_methods'
        )
      ),
      allowOrigins: HtsgetLambdaStack.convertCors(
        configToml,
        'ticket_server_cors_allow_origins'
      ),
      exposeHeaders: HtsgetLambdaStack.convertCors(
        configToml,
        'ticket_server_cors_expose_headers'
      ),
      maxAge:
        configToml.ticket_server_cors_max_age !== undefined
          ? Duration.seconds(configToml.ticket_server_cors_max_age as number)
          : undefined,
      parameterStoreConfig: this.getParameterStoreConfig(config),
    };
  }
}
