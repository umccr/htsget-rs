import { HtsgetSettings } from "../lib/htsget-lambda-stack";

/**
 * Settings to use for the htsget deployment.
 */
export const SETTINGS: HtsgetSettings = {
  config: "config/public_umccr.toml",
  domain: "demo.umccr.org",
  subDomain: "htsget",
  s3BucketResources: [
    "arn:aws:s3:::org.umccr.demo.sbeacon-data/*",
    "arn:aws:s3:::org.umccr.demo.htsget-rs-data/*",
  ],
  lookupHostedZone: true,
};
