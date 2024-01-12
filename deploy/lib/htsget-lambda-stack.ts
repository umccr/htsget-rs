import { STACK_NAME } from "../bin/htsget-lambda";
import * as TOML from "@iarna/toml";
import { readFileSync } from "fs";

import { Duration, Stack, StackProps, Tags } from "aws-cdk-lib";
import { Construct } from "constructs";
import { RustFunction, Settings } from "rust.aws-cdk-lambda";

import { UserPool } from "aws-cdk-lib/aws-cognito";
import {
  Role,
  ServicePrincipal,
  PolicyStatement,
  ManagedPolicy,
} from "aws-cdk-lib/aws-iam";
import { Architecture } from "aws-cdk-lib/aws-lambda";
import {
  CorsHttpMethod,
  HttpMethod,
  HttpApi,
  DomainName,
} from "@aws-cdk/aws-apigatewayv2-alpha";
import { HttpLambdaIntegration } from "@aws-cdk/aws-apigatewayv2-integrations-alpha";
import { HttpJwtAuthorizer } from "@aws-cdk/aws-apigatewayv2-authorizers-alpha";
import {
  Certificate,
  CertificateValidation,
} from "aws-cdk-lib/aws-certificatemanager";
import { ARecord, HostedZone, RecordTarget } from "aws-cdk-lib/aws-route53";
import { ApiGatewayv2DomainProperties } from "aws-cdk-lib/aws-route53-targets";

/**
 * Settings related to the htsget lambda stack.
 */
export type HtsgetLambdaStackSettings = {
  /**
   * The location of the htsget-rs config file.
   */
  config: string;

  /**
   * The domain name for the htsget server.
   */
  domain: string;

  /**
   * Policies to add to the bucket. If this is not specified, this defaults to `["arn:aws:s3:::*"]`.
   * This affects which buckets are allowed to be accessed by the policy actions which are `["s3:List*", "s3:Get*"]`.
   */
  s3BucketResources?: string[];

  /**
   * Whether this deployment is gated behind an authorizer, or if its public. When this is not specified, the htsget
   * api gateway does not have an authorizer.
   */
  authRequired?: HtsgetAuthSettings;

  /**
   * The certificate to use for the domain name. If not specified, a new certificate is created. This is useful if
   * reusing an existing certificate.
   */
  certificateArn?: string;

  /**
   * The hosted zone to use for the htsget domain. If not specified, a new hosted zone is created. This is useful if
   * reusing an existing hosted zone.
   */
  hostedZone?: HtsgetHostedZoneSettings;
};

/**
 * Hosted zone related settings.
 */
export type HtsgetHostedZoneSettings = {
  /**
   * Hosted zone name.
   */
  name: string;
  /**
   * Hosted zone id.
   */
  id: string;
};

/**
 * Authentication related settings.
 */
export type HtsgetAuthSettings = {
  /**
   * The JWT audience.
   */
  jwtAudience: string[];

  /**
   * The cognito user pool id for the authorizer. If this is not set, then a new user pool is created.
   */
  cogUserPoolId?: string;
};

/**
 * Configuration for htsget-rs.
 */
export type Config = {
  /**
   * The config values passed to the htsget-rs server.
   */
  htsgetConfig: { [key: string]: string };

  /**
   * CORS allow credentials.
   */
  allowCredentials?: boolean;

  /**
   * CORS allow headers.
   */
  allowHeaders?: string[];

  /**
   * CORS allow methods.
   */
  allowMethods?: CorsHttpMethod[];

  /**
   * CORS allow origins.
   */
  allowOrigins?: string[];

  /**
   * CORS expose headers.
   */
  exposeHeaders?: string[];

  /**
   * CORS max age.
   */
  maxAge?: Duration;
};

/**
 * Stack used to deploy htsget-lambda.
 */
export class HtsgetLambdaStack extends Stack {
  constructor(
    scope: Construct,
    id: string,
    props: StackProps,
    settings: HtsgetLambdaStackSettings,
  ) {
    super(scope, id, props);

    Tags.of(this).add("Stack", STACK_NAME);

    const config = this.getConfig(settings.config);

    const lambdaRole = new Role(this, id + "Role", {
      assumedBy: new ServicePrincipal("lambda.amazonaws.com"),
      description: "Lambda execution role for " + id,
    });

    const s3BucketPolicy = new PolicyStatement({
      actions: ["s3:List*", "s3:Get*"],
      resources: settings.s3BucketResources ?? ["arn:aws:s3:::*"],
    });

    lambdaRole.addManagedPolicy(
      ManagedPolicy.fromAwsManagedPolicyName(
        "service-role/AWSLambdaBasicExecutionRole",
      ),
    );
    lambdaRole.addToPolicy(s3BucketPolicy);

    // Set the workspace directory of htsget.
    Settings.WORKSPACE_DIR = "../";
    // Don't build htsget packages other than htsget-lambda.
    Settings.BUILD_INDIVIDUALLY = true;

    let htsgetLambda = new RustFunction(this, id + "Function", {
      // Build htsget-lambda only.
      package: "htsget-lambda",
      target: "aarch64-unknown-linux-gnu",

      memorySize: 128,
      timeout: Duration.seconds(28),
      environment: {
        ...config.htsgetConfig,
        RUST_LOG:
          "info,htsget_http_lambda=trace,htsget_config=trace,htsget_http_core=trace,htsget_search=trace",
      },
      features: ["s3-storage"],
      buildEnvironment: {
        RUSTFLAGS: "-C target-cpu=neoverse-n1",
        CARGO_PROFILE_RELEASE_LTO: "true",
        CARGO_PROFILE_RELEASE_CODEGEN_UNITS: "1",
      },
      architecture: Architecture.ARM_64,
      role: lambdaRole,
    });

    const httpIntegration = new HttpLambdaIntegration(
      id + "HtsgetIntegration",
      htsgetLambda,
    );

    // Add an authorizer if auth is required.
    let authorizer = undefined;
    if (settings.authRequired) {
      // If the cog user pool id is not specified, create a new one.
      if (settings.authRequired.cogUserPoolId === undefined) {
        const pool = new UserPool(this, "userPool", {
          userPoolName: "HtsgetRsUserPool",
        });
        settings.authRequired.cogUserPoolId = pool.userPoolId;
      }

      authorizer = new HttpJwtAuthorizer(
        id + "HtsgetAuthorizer",
        `https://cognito-idp.${this.region}.amazonaws.com/${settings.authRequired.cogUserPoolId}`,
        {
          identitySource: ["$request.header.Authorization"],
          jwtAudience: settings.authRequired.jwtAudience,
        },
      );
    }

    let hostedZone;
    if (settings.hostedZone === undefined) {
      hostedZone = new HostedZone(this, id + "HtsgetHostedZone", {
        zoneName: settings.domain,
      });
    } else {
      hostedZone = HostedZone.fromHostedZoneAttributes(
        this,
        id + "HtsgetHostedZone",
        {
          hostedZoneId: settings.hostedZone.id,
          zoneName: settings.hostedZone.name,
        },
      );
    }

    let certificate;
    if (settings.certificateArn === undefined) {
      certificate = new Certificate(this, id + "HtsgetCertificate", {
        domainName: settings.domain,
        validation: CertificateValidation.fromDns(hostedZone),
        certificateName: settings.domain,
      });
    } else {
      certificate = Certificate.fromCertificateArn(
        this,
        id + "HtsgetDomainCert",
        settings.certificateArn,
      );
    }
    const domainName = new DomainName(this, id + "HtsgetDomainName", {
      certificate: certificate,
      domainName: settings.domain,
    });

    new ARecord(this, id + "HtsgetARecord", {
      zone: hostedZone,
      recordName: "htsget",
      target: RecordTarget.fromAlias(
        new ApiGatewayv2DomainProperties(
          domainName.regionalDomainName,
          domainName.regionalHostedZoneId,
        ),
      ),
    });

    const httpApi = new HttpApi(this, id + "ApiGw", {
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

    httpApi.addRoutes({
      path: "/{proxy+}",
      methods: [HttpMethod.GET, HttpMethod.POST],
      integration: httpIntegration,
    });
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
      (value.toString().toLowerCase() === "all" ||
        value.toString().toLowerCase() === "mirror")
    ) {
      return ["*"];
    } else if (Array.isArray(value)) {
      return value;
    }

    return undefined;
  }

  /**
   * Convert a string CORS allowMethod option to CorsHttpMethod.
   */
  static corsAllowMethodToHttpMethod(
    corsAllowMethod?: string[],
  ): CorsHttpMethod[] | undefined {
    if (corsAllowMethod?.length === 1 && corsAllowMethod.includes("*")) {
      return [CorsHttpMethod.ANY];
    } else {
      return corsAllowMethod?.map(
        (element) => CorsHttpMethod[element as keyof typeof CorsHttpMethod],
      );
    }
  }

  /**
   * Get the environment from config.toml
   */
  getConfig(config: string): Config {
    const configToml = TOML.parse(readFileSync(config).toString());

    return {
      htsgetConfig: HtsgetLambdaStack.configToEnv(configToml),
      allowCredentials:
        configToml.ticket_server_cors_allow_credentials as boolean,
      allowHeaders: HtsgetLambdaStack.convertCors(
        configToml,
        "ticket_server_cors_allow_headers",
      ),
      allowMethods: HtsgetLambdaStack.corsAllowMethodToHttpMethod(
        HtsgetLambdaStack.convertCors(
          configToml,
          "ticket_server_cors_allow_methods",
        ),
      ),
      allowOrigins: HtsgetLambdaStack.convertCors(
        configToml,
        "ticket_server_cors_allow_origins",
      ),
      exposeHeaders: HtsgetLambdaStack.convertCors(
        configToml,
        "ticket_server_cors_expose_headers",
      ),
      maxAge:
        configToml.ticket_server_cors_max_age !== undefined
          ? Duration.seconds(configToml.ticket_server_cors_max_age as number)
          : undefined,
    };
  }
}
