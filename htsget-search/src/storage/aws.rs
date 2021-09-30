//! Module providing an implementation for the [Storage] trait using Amazon's S3 object storage service.
use std::path::PathBuf;
use std::env;

use std::convert::TryInto;
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
use rusoto_s3::{
  S3,
  S3Client,
  HeadObjectRequest,
  util::PreSignedRequest,
};

//use super::{GetOptions, Result, Storage, UrlOptions};
//use super::Result;

enum Retrieval {
  Immediate,
  Delayed,
}

// TODO: Encode object "" more statically in this enum?
enum AwsS3StorageTier {
  Standard(Retrieval),
  StandardIa(Retrieval),
  OnezoneIa(Retrieval),
  Glacier(Retrieval),     // ~24-48 hours
  DeepArchive(Retrieval), // ~48 hours
}

/// Implementation for the [Storage] trait using the local file system.
pub struct AwsS3Storage {
  bucket: String,
  key: String,
  region: Region,
  tier: AwsS3StorageTier,
}

impl AwsS3Storage {
  fn new(bucket: String, key: String, region: Region, tier: AwsS3StorageTier) -> Self {
    AwsS3Storage {
      bucket,
      key,
      region,
      tier,
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

  // TODO: Take into account all S3 URL styles...: https://gist.github.com/bh1428/c30b7db493828ece622a6cb38c05362a
  async fn get_bucket_and_key_from_s3_url(s3_url: String) -> Result<(String, String)> {
    let parts: Vec<&str> = s3_url.split_terminator("/").collect();
    Ok((parts[2].to_string(), parts[3].to_string()))
  }

  async fn s3_presign_url(client: S3Client, bucket: String, key: String) -> Result<String> {
    let region = Region::ApSoutheast2;
    let req = s3::GetObjectRequest {
      bucket,
      key,
      ..Default::default()
    };
    let credentials = DefaultCredentialsProvider::new()
      .unwrap()
      .credentials()
      .await
      .unwrap();
    //PreSignedRequestOption expires_in: 3600
    Ok(req.get_presigned_url(&region, &credentials, &Default::default()))
  }

  async fn s3_head_url(client: S3Client, bucket: String, key: String) -> Result<u64> {
    let head_req = HeadObjectRequest {
      bucket: bucket.clone(),
      key: key.clone(),
      ..Default::default()
    };

    Ok(client.head_object(head_req).await?.content_length.unwrap_or(0).try_into().unwrap())
  }

  async fn determine_retrievability() -> Result<AwsS3StorageTier> {
    // 1. S3 head request to object
    // 2. Return status
    unimplemented!();
  }
}

#[async_trait]
impl AsyncStorage for AwsS3Storage {
  async fn get<K: AsRef<str> + Send>(&self, key: K, _options: GetOptions) -> Result<PathBuf> {
    let key: &str = key.as_ref();
    let (bucket, s3key) = AwsS3Storage::get_bucket_and_key_from_s3_url(key.to_string()).await?;

    let s3path = PathBuf::from(bucket).join(s3key);

    Ok(s3path)
  }

  async fn url<K: AsRef<str> + Send>(&self, key: K, options: UrlOptions) -> Result<Url> {
    let client = S3Client::new(Region::default());

    let presigned_url = AwsS3Storage::s3_presign_url(client, self.bucket.clone(), key.as_ref().to_string());
    let htsget_url = Url::new(presigned_url.await?);
    Ok(htsget_url)
  }

  async fn head<K: AsRef<str> + Send>(&self, key: K) -> Result<u64> {
    let key: &str = key.as_ref(); // input URI or path, not S3 key... the trait naming is a bit misleading
    let client = S3Client::new(Region::default());

    let (bucket, s3key) = AwsS3Storage::get_bucket_and_key_from_s3_url(key.to_string()).await?;

    let object_bytes = AwsS3Storage::s3_head_url(client, self.bucket.clone(), s3key).await?;
    Ok(object_bytes)
  }
}

// Testing

struct TestS3Client {
  region: Region,
  s3: S3Client,
  bucket_name: String,
  // This flag signifies whether this bucket was already deleted as part of a test
  bucket_deleted: bool,
}

// impl TestS3Client {
//   // construct S3 testing client
//   fn new(bucket_name: String) -> TestS3Client {
//       let region = if let Ok(endpoint) = env::var("S3_ENDPOINT") {
//           let region = Region::Custom {
//               name: "us-east-1".to_owned(),
//               endpoint: endpoint.to_owned(),
//           };
//           println!(
//               "picked up non-standard endpoint {:?} from S3_ENDPOINT env. variable",
//               region
//           );
//           region
//       } else {
//           Region::UsEast1
//       };

//       TestS3Client {
//           region: region.to_owned(),
//           s3: S3Client::new(region),
//           bucket_name: bucket_name.to_owned(),
//           bucket_deleted: false,
//       }
//   }
// }

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn split_s3_url_into_bucket_and_key() {
    let s3_url = "s3://bucket/key";

    let (bucket, key) = AwsS3Storage::get_bucket_and_key_from_s3_url(s3_url.to_string())
      .await
      .unwrap();

    let s3_storage = AwsS3Storage::new(
      bucket.clone(),
      key.clone(),
      Region::ApSoutheast2,
      AwsS3StorageTier::Standard(Retrieval::Immediate),
    );

    assert_eq!(bucket, "bucket");
    assert_eq!(key, "key");
  }

  #[tokio::test]
  async fn get_htsget_url_from_s3() {
    let s3_url = "s3://bucket/key";

    let (bucket, key) = AwsS3Storage::get_bucket_and_key_from_s3_url(s3_url.to_string())
      .await
      .unwrap();

    let s3_storage = AwsS3Storage::new(
      bucket.clone(),
      key.clone(),
      Region::ApSoutheast2,
      AwsS3StorageTier::Standard(Retrieval::Immediate),
    );

    dbg!(s3_storage.url(key, UrlOptions::default()).await.unwrap());
    // TODO: Assert that the URL is valid https/AWS presigned URL
  }

  #[tokio::test]
  async fn get_head_bytes_from_s3() {
    let s3_url = "s3://bucket/key";

    let (bucket, key) = AwsS3Storage::get_bucket_and_key_from_s3_url(s3_url.to_string())
      .await
      .unwrap();

    let s3_storage = AwsS3Storage::new(
      bucket.clone(),
      key.clone(),
      Region::ApSoutheast2,
      AwsS3StorageTier::Standard(Retrieval::Immediate),
    );

    s3_storage.head(s3_url);
    // TODO: Assert that the the bytes returned by head are expected
  }
}
