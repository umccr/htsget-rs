import { HtsgetSettings } from "../lib/htsget-lambda-stack";

/**
 * Settings to use for the htsget deployment.
 */
export const SETTINGS: HtsgetSettings = {
  config: "config/example_deploy.toml",
  // Specify the domain to serve htsget-rs under.
  domain: "dev.umccr.org",
  subDomain: "htsget",
  s3BucketResources: [],
  lookupHostedZone: false,
  createS3Bucket: true,
  copyTestData: true,
  // Override the bucket name.
  // bucketName: "bucket",
  jwtAuthorizer: {
    // Set this to false if you want a private instance.
    public: false,
    // jwtAudience: ["audience"],
    // cogUserPoolId: "user-pool-id",
  },
  // Enable additional features for compiling htsget-rs. `s3-storage` is always enabled.
  features: ["experimental"],
  copyExampleKeys: true,
};
