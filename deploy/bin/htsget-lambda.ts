import * as cdk from 'aws-cdk-lib';
import { Construct } from 'constructs';
import {HtsgetLambdaConstruct} from "../../deploy/lib/htsget-lambda-construct";

export class HtsgetTestStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    new HtsgetLambdaConstruct(this, 'Id', {
      config: "",
      domain: "",
      s3BucketResources: [],
      jwtAuthorizer: {
        issuer: "your-issuer",
        audience: ["your-audience"],
      },
    });
  }
}