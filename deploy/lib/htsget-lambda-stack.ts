import { Duration, Stack, StackProps, Tags } from "aws-cdk-lib";
import { Construct } from "constructs";
import * as iam from "aws-cdk-lib/aws-iam";
import { RustFunction, Settings } from "rust.aws-cdk-lambda";
import { Architecture } from "aws-cdk-lib/aws-lambda";
import * as apigwv2 from "@aws-cdk/aws-apigatewayv2-alpha";
import { STACK_NAME } from "../bin/htsget-lambda";
import { HttpLambdaIntegration } from "@aws-cdk/aws-apigatewayv2-integrations-alpha";
import { HttpJwtAuthorizer } from "@aws-cdk/aws-apigatewayv2-authorizers-alpha";
import { ARecord, HostedZone, RecordTarget } from "aws-cdk-lib/aws-route53";
import { ApiGatewayv2DomainProperties } from "aws-cdk-lib/aws-route53-targets";
import { Certificate, CertificateValidation } from "aws-cdk-lib/aws-certificatemanager";
import * as fs from "fs";
import * as TOML from "@iarna/toml";
import { CognitoUserPoolsAuthorizer, DomainName } from "aws-cdk-lib/aws-apigateway";
import { aws_cognito } from "aws-cdk-lib";

/**
 * Configuration for HtsgetLambdaStack.
 */
export type Config = {
  environment: string;                          // Dev, prod, public
  htsgetConfig: { [key: string]: any };         // Server config
  allowCredentials?: boolean;                   // CORS
  allowHeaders?: string[];
  allowMethods?: apigwv2.CorsHttpMethod[];
  allowOrigins?: string[];
  exposeHeaders?: string[];
  maxAge?: Duration;
  authRequired?: boolean;                       // Public instance without authz/n
  rateLimits?: boolean;                         // Reasonable defaults or configurable ratelimit settings?
  //arnCert: string;                            // TODO: Needs to be fetched from the recently created certificate
  //hostedZoneId?: string;                       // TODO: Ditto above
  hostedZoneName?: string;                     // TODO: Ditto above
  cogUserPoolId?: string;                       // Supply one if already existing
  //jwtAud: string[];                           // TODO: Ditto above
  //htsgetDomain: string;                       // TODO: Fetched from the TOML file
};

/**
 * Stack used to deploy htsget-lambda.
 */
export class HtsgetLambdaStack extends Stack {
  // Read config from cdk.json and TOML file(s).
  config = this.getConfig();

  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    const config = this.config;

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
      htsgetLambda
    );

    // Use a predefined Cognito user pool or create a new one.
    var cognito = undefined;
    if (!config.authRequired || config.cogUserPoolId) {
      Error("Cognito user pool requested by {toml.cognito_name} not found");
      cognito = config.cogUserPoolId;
    } else {

      // TODO: Creating a new one
    }

    // Use a predefined authorizer or create a new one.
    var authorizer = undefined;
    if (config.authRequired) {
      authorizer = new HttpJwtAuthorizer(
      id + "HtsgetAuthorizer",
      `https://cognito-idp.${this.region}.amazonaws.com/${config.cogUserPoolId}`,
        {
          identitySource: ["$request.header.Authorization"],
          jwtAudience: ["foobar"] // TODO: Fetch from newly created resource by this stack (instead of SSM)
        }
      )
    }

    const hostedZoneObj = HostedZone.fromLookup(
      this,
      id + "HtsgetHostedZone",
      {
        domainName: config.htsgetConfig.HTSGET_DOMAIN,
      }
    );

    // Create a certificate for the domain name.
    const certificateArn = new Certificate(
      this,
      id + "HtsgetCertificate",
      {
        domainName: config.htsgetConfig.HTSGET_DOMAIN,
        validation: CertificateValidation.fromDns(hostedZoneObj),
        certificateName: config.htsgetConfig.HTSGET_DOMAIN,
      }
    ).certificateArn;

    console.log(config.htsgetConfig);
    // Create a domain name for the API Gateway.
    const domainName = new apigwv2.DomainName(this, id + "HtsgetDomainName", {
      certificate: Certificate.fromCertificateArn(
        this,
        id + "HtsgetDomainCert",
        //domainName: config.htsgetConfig.domainName,
        certificateArn
      ),
      domainName: config.htsgetConfig.HTSGET_DOMAIN,
    });

    // TODO: Use the hosted zone from the certificate
    const hostedZone = HostedZone.fromHostedZoneAttributes(
      this,
      id + "HtsgetHostedZone",
      {
        hostedZoneId: config.htsgetConfig.hostedZoneId,
        zoneName: config.htsgetConfig.hostedZoneName,
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

    const httpApi = new apigwv2.HttpApi(this, id + "ApiGw", {
      // Use explicit routes GET, POST with {proxy+} path
      // defaultIntegration: httpIntegration,
      defaultAuthorizer: config.authRequired ? authorizer : undefined,
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
      methods: [apigwv2.HttpMethod.GET, apigwv2.HttpMethod.POST],
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
    corsAllowMethod?: string[]
  ): apigwv2.CorsHttpMethod[] | undefined {
    if (corsAllowMethod?.length === 1 && corsAllowMethod.includes("*")) {
      return [apigwv2.CorsHttpMethod.ANY];
    } else {
      return corsAllowMethod?.map(
        (element) =>
          apigwv2.CorsHttpMethod[element as keyof typeof apigwv2.CorsHttpMethod]
      );
    }
  }

  /**
   * Get the environment from config.toml
   */
  getConfig(): Config {
    let env: string = this.node.tryGetContext("env");

    const config = this.node.tryGetContext(env);
    // TODO: Remove hardcoding, parametrize this better for the different environments
    const configToml = TOML.parse(fs.readFileSync("config/public_umccr.toml").toString());
    //console.log(configToml);
    return {
      environment: env,
      htsgetConfig: HtsgetLambdaStack.configToEnv(configToml),
      allowCredentials:
        configToml.ticket_server_cors_allow_credentials as boolean,
      allowHeaders: HtsgetLambdaStack.convertCors(
        configToml,
        "ticket_server_cors_allow_headers"
      ),
      allowMethods: HtsgetLambdaStack.corsAllowMethodToHttpMethod(
        HtsgetLambdaStack.convertCors(
          configToml,
          "ticket_server_cors_allow_methods"
        )
      ),
      allowOrigins: HtsgetLambdaStack.convertCors(
        configToml,
        "ticket_server_cors_allow_origins"
      ),
      exposeHeaders: HtsgetLambdaStack.convertCors(
        configToml,
        "ticket_server_cors_expose_headers"
      ),
      maxAge:
        configToml.ticket_server_cors_max_age !== undefined
          ? Duration.seconds(configToml.ticket_server_cors_max_age as number)
          : undefined,
      // authRequired:
      // rateLimits:
    };
  }
}
