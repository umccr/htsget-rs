//! Module providing an implementation for the [Storage] trait using Amazon's S3 object storage service.
use std::path::{ PathBuf };

use async_trait::async_trait;
use std::convert::TryInto;

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
use rusoto_s3::{util::PreSignedRequest, HeadObjectRequest, S3Client, S3};

use crate::storage::s3_testing::fs_write_object;

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

    Ok(
      client
        .head_object(head_req)
        .await?
        .content_length
        .unwrap_or(0)
        .try_into()
        .unwrap(),
    )
  }

  async fn determine_retrievability() -> Result<AwsS3StorageTier> {
    // 1. S3 head request to object
    // 2. Return status
    // Similar (Java) code I wrote here: https://github.com/igvteam/igv/blob/master/src/main/java/org/broad/igv/util/AmazonUtils.java#L257
    // Or with AWS cli with: $ aws s3api head-object --bucket awsexamplebucket --key dir1/example.obj
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

    let presigned_url =
      AwsS3Storage::s3_presign_url(client, self.bucket.clone(), key.as_ref().to_string());
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


#[cfg(test)]
mod tests {
  use super::*;

  use crate::storage::s3_testing::setup_service;
  use crate::storage::s3_testing::recv_body_string;

  use hyper::{Body, Method, StatusCode};
  use s3_server::headers::HeaderValue;
  use s3_server::headers::X_AMZ_CONTENT_SHA256;

  type Request = hyper::Request<hyper::Body>;

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

    let htsget_url = s3_storage.url(key, UrlOptions::default()).await.unwrap();
    //dbg!(&htsget_url);
    assert!(htsget_url.url.contains("X-Amz-Signature"));
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

    //let htsget_head = s3_storage.head(s3_url).await.unwrap();
    //dbg!(htsget_head);
    //assert!(htsget_head.is_ok());
  }

  #[tokio::test]
  async fn get_local_s3_server_object() {
      let (root, service) = setup_service().unwrap();

      let bucket = "asd";
      let key = "qwe";
      let content = "Hello World!";

      fs_write_object(root, bucket, key, content).unwrap();

      let mut req = Request::new(Body::empty());
      *req.method_mut() = Method::GET;
      *req.uri_mut() = format!("http://localhost/{}/{}", bucket, key)
          .parse()
          .unwrap();
      req.headers_mut().insert(
          X_AMZ_CONTENT_SHA256.clone(),
          HeaderValue::from_static("UNSIGNED-PAYLOAD"),
      );

      let mut res = service.hyper_call(req).await.unwrap();
      let body = recv_body_string(&mut res).await.unwrap();

      assert_eq!(res.status(), StatusCode::OK);
      assert_eq!(body, content);
  }

  // #[tokio::test]
  // async fn local_s3_server_returns_htsget_url() {
  //   let (root, service) = setup_service().unwrap();

  //   let bucket = "bucket";
  //   let key = "key";
  //   let content = "Hello World!";

  // }
}