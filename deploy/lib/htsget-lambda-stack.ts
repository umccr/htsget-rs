import { Duration, Stack, StackProps, Tags } from "aws-cdk-lib";
import { Construct } from "constructs";
import * as iam from "aws-cdk-lib/aws-iam";
import { RustFunction, Settings } from "rust.aws-cdk-lambda";
import { Architecture } from "aws-cdk-lib/aws-lambda";
import * as apigwv2 from "@aws-cdk/aws-apigatewayv2-alpha";
import { STACK_NAME } from "../bin/htsget-lambda";
import { HttpLambdaIntegration } from "@aws-cdk/aws-apigatewayv2-integrations-alpha";
import { StringParameter } from "aws-cdk-lib/aws-ssm";
import { HttpJwtAuthorizer } from "@aws-cdk/aws-apigatewayv2-authorizers-alpha";
import { ARecord, HostedZone, RecordTarget } from "aws-cdk-lib/aws-route53";
import { ApiGatewayv2DomainProperties } from "aws-cdk-lib/aws-route53-targets";
import { Certificate } from "aws-cdk-lib/aws-certificatemanager";
import * as fs from "fs";
import * as toml from "toml";

/**
 * Configuration for HtsgetLambdaStack.
 */
export type Config = {
  environment: string;
  config: { [key: string]: any };
  cors_allow_origins: string[];
};

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
};

/**
 * Stack used to deploy htsget-lambda.
 */
export class HtsgetLambdaStack extends Stack {
  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    Tags.of(this).add("Stack", STACK_NAME);

    const lambdaRole = new iam.Role(this, id + "Role", {
      assumedBy: new iam.ServicePrincipal("lambda.amazonaws.com"),
      description: "Lambda execution role for " + id,
    });

    const s3BucketPolicy = new iam.PolicyStatement({
      actions: ["s3:List*", "s3:Get*"],
      resources: ["arn:aws:s3:::*"],
    });

    lambdaRole.addManagedPolicy(
      iam.ManagedPolicy.fromAwsManagedPolicyName(
        "service-role/AWSLambdaBasicExecutionRole"
      )
    );
    lambdaRole.addToPolicy(s3BucketPolicy);

    // Set the workspace directory of htsget.
    Settings.WORKSPACE_DIR = "../";
    // Don't build htsget packages other than htsget-lambda.
    Settings.BUILD_INDIVIDUALLY = true;

    const config = this.getConfig();
    let htsgetLambda = new RustFunction(this, id + "Function", {
      // Build htsget-lambda only.
      package: "htsget-lambda",
      target: "aarch64-unknown-linux-gnu",

      memorySize: 128,
      timeout: Duration.seconds(10),
      environment: { ...config.config },
      architecture: Architecture.ARM_64,
      role: lambdaRole,
    });

    const ssmConfig = this.getSSMConfig();
    const httpIntegration = new HttpLambdaIntegration(
      id + "HtsgetIntegration",
      htsgetLambda
    );
    const authorizer = new HttpJwtAuthorizer(
      id + "HtsgetAuthorizer",
      `https://cognito-idp.${this.region}.amazonaws.com/${ssmConfig.cog_user_pool_id}`,
      {
        identitySource: ["$request.header.Authorization"],
        jwtAudience: [
          ssmConfig.cog_app_client_id_stage,
          ssmConfig.cog_app_client_id_local,
        ],
      }
    );

    const domainName = new apigwv2.DomainName(this, id + "HtsgetDomainName", {
      certificate: Certificate.fromCertificateArn(
        this,
        id + "HtsgetDomainCert",
        ssmConfig.cert_apse2_arn
      ),
      domainName: ssmConfig.domain_name,
    });
    const hostedZone = HostedZone.fromHostedZoneAttributes(
      this,
      id + "HtsgetHostedZone",
      {
        hostedZoneId: ssmConfig.hosted_zone_id,
        zoneName: ssmConfig.hosted_zone_name,
      }
    );
    new ARecord(this, id + "HtsgetARecord", {
      zone: hostedZone,
      recordName: "htsget",
      target: RecordTarget.fromAlias(
        new ApiGatewayv2DomainProperties(
          domainName.regionalDomainName,
          domainName.regionalHostedZoneId
        )
      ),
    });

    new apigwv2.HttpApi(this, id + "ApiGw", {
      defaultIntegration: httpIntegration,
      defaultAuthorizer: authorizer,
      defaultDomainMapping: {
        domainName: domainName,
      },
      corsPreflight: {
        allowOrigins: config.cors_allow_origins,
        allowHeaders: ["*"],
        allowMethods: [apigwv2.CorsHttpMethod.ANY],
        allowCredentials: true,
      },
    });
  }

  /**
   * Get config values from SSM.
   */
  getSSMConfig(): SSMConfig {
    return {
      cert_apse2_arn: StringParameter.valueFromLookup(
        this,
        "/htsget/acm/apse2_arn"
      ),
      cog_app_client_id_local: StringParameter.valueFromLookup(
        this,
        "/data_portal/client/cog_app_client_id_local"
      ),
      cog_app_client_id_stage: StringParameter.valueFromLookup(
        this,
        "/data_portal/client/cog_app_client_id_stage"
      ),
      cog_user_pool_id: StringParameter.valueFromLookup(
        this,
        "/data_portal/client/cog_user_pool_id"
      ),
      domain_name: StringParameter.valueFromLookup(this, "/htsget/domain"),
      hosted_zone_id: StringParameter.valueFromLookup(this, "hosted_zone_id"),
      hosted_zone_name: StringParameter.valueFromLookup(
        this,
        "hosted_zone_name"
      ),
    };
  }

  /**
   * Convert JSON config to htsget-rs env representation.
   */
  static configToEnv(config: any): { [key: string]: string } {
    const out: { [key: string]: string } = {};
    for (const key in config) {
      out[`HTSGET_${key.toUpperCase()}`] = JSON.stringify(
        config[key]
      ).replaceAll(":", "=");
    }
    return out;
  }

  /**
   * Get the environment configuration from cdk.json. Pass `--context "env=dev"` or `--context "env=prod"` to
   * control the environment.
   */
  getConfig(): Config {
    let env: string = this.node.tryGetContext("env");
    if (!env) {
      console.log("No environment supplied, using `dev` environment config");
      env = "dev";
    }

    const config = this.node.tryGetContext(env);
    const configToml = toml.parse(fs.readFileSync(config.config).toString());

    return {
      environment: env,
      config: HtsgetLambdaStack.configToEnv(configToml),
      cors_allow_origins: config.cors_allow_origins,
    };
  }
}
