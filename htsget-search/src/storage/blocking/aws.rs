//! Module providing an implementation for the [Storage] trait using Amazon's S3 object storage service.
//!
use aws_sdk_s3 as s3;

use crate::htsget::{Headers, Url};

use super::{GetOptions, Result, Storage, UrlOptions};

// TODO: Encode object "reachability" in this enum?
enum AwsStorageTier {
    Standard,
    StandardIa,
    OnezoneIa,
    Glacier,
    DeepArchive,
}

/// Implementation for the [Storage] trait using the local file system.
#[derive(Debug)]
pub struct AwsS3Storage {
  prefix: String,
  key: String,
  region: String,
  presigned_url: String,
  tier: AwsStorageTier,
}

// TODO: Use S3 shared client ref?
impl AwsS3Storage {
  pub fn new<S3Url: ToString>(&self, s3_url: ToString) -> Result<Self> {
    
    // TODO: Simple prefix/key splitting method given an S3 url will be required 

    let conf = s3::Config::builder()
                    .region(s3::Region::Custom(self.region.to_string()))
                    .build();
    let client = s3::Client::from_conf(conf);

    AwsS3Storage { 
            prefix: self.prefix,
            key: self.key, 
            region: self.region,
            presigned_url: "TBD_presign".to_string(), // TODO: not supported yet upstream, see https://github.com/awslabs/aws-sdk-rust/issues/139
            tier: AwsStorageTier::STANDARD, // Assume STANDARD as reasonable default
    }
  }

  pub fn tier(&self) -> &AwsStorageTier {
      self.tier
  }
  pub fn key(&self) -> String {
    self.key
  }

  // https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-query-string-auth.html
  // https://github.com/rusoto/rusoto/commit/d6bec67727bdef713dd568a8cf340dc001dda719#diff-4a2fd6328056ba5189dc34f76e9ea94f1fd35c4f69e014916a85f48caaac36a4R69
  // https://github.com/rusoto/rusoto/commit/d6bec67727bdef713dd568a8cf340dc001dda719#diff-185327ef0623037e62d849db265da7b671f33672666199fb80dfc2ec80f4b036R234
  fn presign_url(&self, prefix: String, key: String) -> Result<String> { Ok("TBD_presign".to_string()) }
}

impl Storage for AwsS3Storage {
  fn get<K: AsRef<str>>(&self, s3_url: K, _options: GetOptions) -> Result<Url> {
    self.presign_url(s3_url)
  }

  fn url<K: AsRef<str>>(&self, s3_url: K, options: UrlOptions) -> Result<Url> {
    let range_start = options
      .range
      .start
      .map(|start| start.to_string())
      .unwrap_or_else(|| "".to_string());
    let range_end = options
      .range
      .end
      .map(|end| end.to_string())
      .unwrap_or_else(|| "".to_string());

    // TODO: Return S3 presigned URLs with the range requests
    let url = Url::new(self.presign_url(s3_url)); // TODO: Pass ranges too, depending on SDK's method signature
    let url = if range_start.is_empty() && range_end.is_empty() {
      url
    } else {
      url.with_headers(
        Headers::default().with_header("Range", format!("bytes={}-{}", range_start, range_end)),
      )
    };
    let url = url.with_class(options.class);
    Ok(url)
  }

  fn head<K: AsRef<str>>(&self, s3_url: K) -> Result<u64> {
    let aws_storage = AwsS3Storage::new(s3_url);
    let url = aws_storage.presign_url(s3_url);
    Ok(
      url.get_response().head()
    )
  }
}