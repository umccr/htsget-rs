import { HtsgetSettings } from "../lib/htsget-lambda-stack";

/**
 * Settings to use for the htsget deployment.
 */
export const SETTINGS: HtsgetSettings = {
  config: "config/dev_umccr.toml",
  domain: "dev.umccr.org",
  subDomain: "htsget",
  s3BucketResources: [
    "arn:aws:s3:::org.umccr.demo.sbeacon-data/*",
    "arn:aws:s3:::org.umccr.demo.htsget-rs-data/*",
  ],
  lookupHostedZone: true,
  jwtAuthorizer: {
    // Set this to true if you want a public instance.
    public: false,
    // jwtAudience: ["audience"],
    // cogUserPoolId: "user-pool-id"
  },
};
