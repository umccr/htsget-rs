//! Module providing an implementation for the [Storage] trait using Amazon's S3 object storage service.
//!
use std::path::PathBuf;

use async_trait::async_trait;

use crate::htsget::Url;

use super::{GetOptions, Result, UrlOptions};
use crate::storage::async_storage::AsyncStorage;
//#[cfg(feature = "aws_rust_sdk")]
//use aws_sdk_s3 as s3;
//#[cfg(feature = "rusoto")]
use rusoto_core::{
  credential::{DefaultCredentialsProvider, ProvideAwsCredentials},
  Region,
};
use rusoto_s3 as s3;
use rusoto_s3::util::PreSignedRequest;

//use super::{GetOptions, Result, Storage, UrlOptions};
//use super::Result;

// TODO: Use envy for AWS creds?
// TODO: Encode object "reachability" in this enum?
enum Reachability {
  ImmediateRetrieval,
  DelayedRetrieval,
}

enum AwsStorageTier {
  Standard(Reachability),
  StandardIa(Reachability),
  OnezoneIa(Reachability),
  Glacier(Reachability),      // ~24-48 hours
  DeepArchive(Reachability),  // ~48 hours
}

/// Implementation for the [Storage] trait using the local file system.
pub struct AwsS3Storage {
  bucket: String,
  key: String,
  region: Region,
  presigned_url: String,
  tier: AwsStorageTier,
}

impl AwsS3Storage {
  fn new(
    bucket: String,
    key: String,
    region: Region,
    presigned_url: String,
    tier: AwsStorageTier,
  ) -> Self {
    AwsS3Storage {
      bucket,
      key,
      region,
      presigned_url,
      tier: tier,
    }
  }
  // TODO: infer region?: https://rusoto.github.io/rusoto/rusoto_s3/struct.GetBucketLocationRequest.html
  fn get_region(&self) -> Region {
    let region = if let Ok(url) = std::env::var("AWS_ENDPOINT_URL") {
      Region::Custom {
        name: std::env::var("AWS_REGION").unwrap_or_else(|_| "custom".to_string()),
        endpoint: url,
      }
    } else {
      Region::default()
    };

    region
  }

  async fn _get_bucket_and_key_from_s3_url(&self, _s3_url: String) -> Result<(String, String)> {
    unimplemented!();
    // https://gist.github.com/bh1428/c30b7db493828ece622a6cb38c05362a
  }

  async fn s3_presign_url(bucket_name: String, key: String) -> String {
    let region = Region::ApSoutheast2;
    let req = s3::GetObjectRequest {
      bucket: bucket_name,
      key: key,
      ..Default::default()
    };
    let credentials = DefaultCredentialsProvider::new()
      .unwrap()
      .credentials()
      .await
      .unwrap();
    //PreSignedRequestOption expires_in: 3600
    req.get_presigned_url(&region, &credentials, &Default::default())
  }

  async fn determine_reachability() -> Result<AwsStorageTier> {
    unimplemented!();
  }
}

#[async_trait]
impl AsyncStorage for AwsS3Storage {
  async fn get<K: AsRef<str> + Send>(&self, key: K, _options: GetOptions) -> Result<PathBuf> {
    //let (bucket, s3key) = self.get_bucket_and_key_from_s3_url(key).await?;
    unimplemented!();
    // Ok(PathBuf::from(key))
  }

  async fn url<K: AsRef<str> + Send>(&self, key: K, options: UrlOptions) -> Result<Url> {
    unimplemented!();
  }

  async fn head<K: AsRef<str> + Send>(&self, key: K) -> Result<u64> {
    Ok(0)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn split_s3_url_into_bucket_and_key() {
    let s3_url = "s3://bucket/key";

    let (bucket, key) = AwsS3Storage::new(&self, s3_url).get_bucket_and_key_from_s3_url(s3_url)?;
    assert_eq!(bucket, "bucket");
    assert_eq!(key, "key");
  }
}
