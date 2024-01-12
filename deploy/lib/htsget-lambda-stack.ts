import { STACK_NAME } from "../bin/htsget-lambda";
import * as TOML from "@iarna/toml";
import { readFileSync } from "fs";

import { Duration, Stack, StackProps, Tags, CfnParameter } from "aws-cdk-lib";
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

// TODO:
//
// * Include CORS snippet in S3 buckets' permissions for the public case, iterate through all buckets to enable CORS.
// * Add a custom domain name for the API gateway, deal with certificates and API gateway Route53 mapping (no CNAME/A's there).
// * Make sure CORS is disabled/removed for the unauthorized case.
// * Revisit Cognito config for the auth'd case.
// * Tweak resolvers regex and substitutions strings, since bam|cram|mixed... does not work currently.
// * Deploy new changes from upstream htsget-rs.
// * Consider CDK Pipelines migration so that commits to the GitHub's `master` repo branch trigger a new deployment.

/**
 * Configuration for HtsgetLambdaStack.
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
  /**
   * The domain name for the htsget server.
   */
  domain: string;
  /**
   * Whether this deployment is gated behind an authorizer, or if its public.
   */
  authRequired: boolean;
  /**
   * The cognito user pool id for the authorizer. If this is not set, then a new user pool is created.
   * This option only has an effect if authRequired is set to true.
   */
  cogUserPoolId?: string;
};

/**
 * Stack used to deploy htsget-lambda.
 */
export class HtsgetLambdaStack extends Stack {
  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    // Read config from cdk.json and TOML file(s).
    const config = this.getConfig();

    Tags.of(this).add("Stack", STACK_NAME);

    const lambdaRole = new Role(this, id + "Role", {
      assumedBy: new ServicePrincipal("lambda.amazonaws.com"),
      description: "Lambda execution role for " + id,
    });

    const s3BucketPolicy = new PolicyStatement({
      actions: ["s3:List*", "s3:Get*"],
      resources: this.configResolversToARNBuckets(config.htsgetConfig),
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
    if (config.authRequired) {
      // If the cog user pool id is not specified, create a new one.
      if (config.cogUserPoolId === undefined) {
        const pool = new UserPool(this, "userPool", {
          userPoolName: "HtsgetRsUserPool",
        });
        config.cogUserPoolId = pool.userPoolId;
      }

      authorizer = new HttpJwtAuthorizer(
        id + "HtsgetAuthorizer",
        `https://cognito-idp.${this.region}.amazonaws.com/${config.cogUserPoolId}`,
        {
          identitySource: ["$request.header.Authorization"],
          jwtAudience: ["audience"],
        },
      );
    }

    // Create a hosted zone for this service.
    const hostedZone = new HostedZone(this, id + "HtsgetHostedZone", {
      zoneName: config.domain,
    });
    // Create a certificate for the domain name.
    const certificate = new Certificate(this, id + "HtsgetCertificate", {
      domainName: config.domain,
      validation: CertificateValidation.fromDns(hostedZone),
      certificateName: config.domain,
    });
    const domainName = new DomainName(this, id + "HtsgetDomainName", {
      certificate: Certificate.fromCertificateArn(
        this,
        id + "HtsgetDomainCert",
        certificate.certificateArn,
      ),
      domainName: config.domain,
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
   * Collect resource names from config.
   * @param config TOML config file
   * @returns A list of buckets (storage backend identifiers or names)
   */
  configResolversToARNBuckets(config: {
    [key: string]: string;
  }): Array<string> {
    // todo
    return [];
    // // Example return value:
    // //  [ "arn:aws:s3:::org.umccr.demo.sbeacon-data/*",
    // //    "arn:aws:s3:::org.umccr.demo.htsget-rs-data/*" ]
    //
    // // Parse the JSON string into a JavaScript object
    // const resolvers = config["HTSGET_RESOLVERS"];
    //
    // // Build a bucket => keys dictionary, for now we'll just need the bucket part for the policies
    // var out: Array<string> = [];
    //
    // const regexPattern = /regex\s*=\s*"\^\(([^/]+)\)\//gm;
    // const matches = resolvers.match(regexPattern);
    //
    // if (matches) {
    //   for (const match of matches) {
    //     out.push(match.replace(regexPattern, "arn:aws:s3:::$1/*"));
    //   }
    // }
    //
    // return out;
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
  getConfig(): Config {
    let confFile = this.node.tryGetContext("htsget_rs_config");

    const configToml = TOML.parse(readFileSync(confFile).toString());

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
      domain: configToml.domain as string,
      authRequired: configToml.auth_required as boolean,
      cogUserPoolId: configToml.cog_user_pool_id as string | undefined,
    };
  }
}
