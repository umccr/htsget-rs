import { HtsgetSettings } from "../lib/htsget-lambda-construct";

/**
 * Settings to use for the htsget deployment.
 */
export const SETTINGS: HtsgetSettings = {
  config: "config/example_deploy.toml",
  // Specify the domain to serve htsget-rs under.
  domain: "dev.umccr.org",
  subDomain: "htsget-c4gh",
  s3BucketResources: [],
  lookupHostedZone: true,
  createS3Bucket: true,
  copyTestData: true,
  // Override the bucket name.
  // bucketName: "bucket",
  jwtAuthorizer: {
    // Set this to false if you want a private instance.
    public: false,
    cogUserPoolId: "ap-southeast-2_iWOHnsurL",
    jwtAudience: ["..."],
    //issuer: "Amazon",
    // jwtAudience: ["audience"],
    // cogUserPoolId: "user-pool-id",
  },
};
