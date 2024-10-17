import * as cdk from 'aws-cdk-lib';
import { Construct } from 'constructs';
import {HtsgetLambdaConstruct} from "../../deploy/lib/htsget-lambda-construct";

export class HtsgetTestStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    new HtsgetLambdaConstruct(this, 'Htsget-rs', {
      config: "",
      domain: "",
      lookupHostedZone: true,
      s3BucketResources: [],
      jwtAuthorizer: {
        public: false,
      },
    });
  }
}