//! Module providing an implementation for the [Storage] trait using Amazon's S3 object storage service.
use std::io::Cursor;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use aws_config::meta::region::RegionProviderChain;
use aws_config::timeout::Config;
use aws_sdk_s3::{Client as S3Client, Region};
use aws_sdk_s3::input::GetObjectInput;
use aws_sdk_s3::operation::GetObject;
use aws_sdk_s3::presigning::config::PresigningConfig;
use aws_types::SdkConfig;
use bytes::Bytes;
use futures::TryStreamExt;
use tokio::io::BufReader;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use htsget_id_resolver::RegexResolver;

use crate::htsget::Url;
use crate::storage::async_storage::AsyncStorage;
use crate::storage::aws::s3_url::parse_s3_url;
use crate::storage::StorageError;

use super::{GetOptions, Result, UrlOptions};

mod s3_testing_helper;
mod s3_url;

//use crate::storage::s3_testing::fs_write_object;

enum Retrieval {
  Immediate,
  Delayed,
}

enum AwsS3StorageTier {
  Standard(Retrieval),
  StandardIa(Retrieval),
  OnezoneIa(Retrieval),
  Glacier(Retrieval),     // ~24-48 hours
  DeepArchive(Retrieval), // ~48 hours
}

/// Implementation for the [Storage] trait utilising data from an S3 bucket.
pub struct AwsS3Storage {
  config: SdkConfig,
  client: S3Client,
  bucket: String,
  id_resolver: RegexResolver,
}

impl AwsS3Storage {
  // Allow the user to set this?
  pub const PRESIGNED_REQUEST_EXPIRY: u64 = 1000;

  pub fn new(config: SdkConfig, bucket: String, id_resolver: RegexResolver) -> Self {
    AwsS3Storage {
      config,
      client: S3Client::new(&config),
      bucket,
      id_resolver
    }
  }

  pub async fn new_with_default_config(bucket: String, id_resolver: RegexResolver) -> Self {
    AwsS3Storage::new(aws_config::load_from_env().await, bucket, id_resolver)
  }

  async fn s3_presign_url<K: AsRef<str> + Send>(&self, key: K) -> Result<String> {
    Ok(
      self
      .client
      .get_object()
      .bucket(&self.bucket)
        .key(key.as_ref())
      .presigned(
        PresigningConfig::expires_in(Duration::from_secs(Self::PRESIGNED_REQUEST_EXPIRY))
          .map_err(|err| StorageError::AwsError(err.to_string(), key.as_ref().to_string()))?
      )
      .await
      .map_err(|err| StorageError::AwsError(err.to_string(), key.as_ref().to_string()))?
      .uri()
      .to_string()
    )
  }

  async fn s3_head<K: AsRef<str> + Send>(&self, key: K) -> Result<u64> {
    let content_length = self.client
      .head_object()
      .bucket(&self.bucket)
      .key(key.as_ref())
      .send()
      .await
      .unwrap()
      .content_length as u64;

    Ok(content_length)
  }

  async fn get_storage_tier<K: AsRef<str> + Send>(key: K) -> Result<AwsS3StorageTier> {
    // 1. S3 head request to object
    // 2. Return status
    // Similar (Java) code I wrote here: https://github.com/igvteam/igv/blob/master/src/main/java/org/broad/igv/util/AmazonUtils.java#L257
    // Or with AWS cli with: $ aws s3api head-object --bucket awsexamplebucket --key dir1/example.obj
    unimplemented!();
  }

  async fn get_content<K: AsRef<str> + Send>(&self, key: K, options: GetOptions) -> Result<Bytes> {
    // It would be nice to use a ready-made type with a ByteStream that implements AsyncRead + AsyncSeek
    // in order to avoid reading the whole byte buffer into memory. A custom type could be made similar to
    // https://users.rust-lang.org/t/what-to-pin-when-implementing-asyncread/63019/2 which could be based off
    // StreamReader.
    let response = self.client
      .get_object()
      .bucket(&self.bucket)
      .key(key.as_ref())
      .send()
      .await
      .map_err(|err| StorageError::AwsError(err.to_string(), key.to_string()))?
      .body
      .collect()
      .await
      .map_err(|err| StorageError::AwsError(err.to_string(), key.to_string()))?
      .into_bytes();

    Ok(response)
  }
}

// TODO: Determine if all three trait methods require Retrievavility testing before
// reaching out to actual S3 objects or just the "head" operation.
// i.e: Should we even return a presigned URL if the object is not immediately retrievable?`
#[async_trait]
impl AsyncStorage for AwsS3Storage {
  type Streamable = BufReader<Cursor<Bytes>>;

  /// Returns the S3 url (s3://bucket/key) for the given path (key).
  async fn get<K: AsRef<str> + Send>(&self, key: K, _options: GetOptions) -> Result<BufReader<Cursor<Bytes>>> {
    let response = self.get_content(key, _options).await?;
    let cursor = Cursor::new(response);
    let reader = tokio::io::BufReader::new(cursor);
    Ok(reader)
  }

  /// Returns a S3-presigned htsget URL
  async fn url<K: AsRef<str> + Send>(&self, key: K, options: UrlOptions) -> Result<Url> {
    let shared_config = Self::get_shared_config().await;
    let client = S3Client::new(&shared_config);

    let presigned_url =
      AwsS3Storage::s3_presign_url(client, self.bucket.clone(), key.as_ref().to_string());
    let htsget_url = Url::new(presigned_url.await?);

    Ok(htsget_url)
  }

  /// Returns the size of the S3 object in bytes.
  async fn head<K: AsRef<str> + Send>(&self, key: K) -> Result<u64> {
    let shared_config = Self::get_shared_config().await;

    let key: &str = key.as_ref(); // input URI or path, not S3 key... the trait naming is a bit misleading
    let client = S3Client::new(&shared_config);

    let (_, s3key, _) = parse_s3_url(key)?;

    let object_bytes = AwsS3Storage::s3_head(client, self.bucket.clone(), s3key).await?;
    Ok(object_bytes)
  }
}

#[cfg(test)]
mod tests {
  use hyper::{Body, Method, StatusCode};
  use s3_server::headers::HeaderValue;
  use s3_server::headers::X_AMZ_CONTENT_SHA256;

  use crate::storage::aws::s3_testing_helper::fs_write_object;
  use crate::storage::aws::s3_testing_helper::recv_body_string;
  use crate::storage::aws::s3_testing_helper::setup_service;

  use super::*;

  type Request = hyper::Request<hyper::Body>;

  async fn aws_s3_client() -> S3Client {
    let region_provider = RegionProviderChain::first_try("ap-southeast-2")
      .or_default_provider()
      .or_else(Region::new("us-east-1"));

    let shared_config = aws_config::from_env().region(region_provider).load().await;

    S3Client::new(&shared_config)
  }

  #[tokio::test]
  async fn test_get_htsget_url_from_s3() {
    let s3_storage = AwsS3Storage::new(aws_s3_client().await, String::from("bucket"));

    let htsget_url = s3_storage.url("key", UrlOptions::default()).await;

    assert!(htsget_url.unwrap().url.contains("X-Amz-Signature"));
  }

  // #[tokio::test]
  // async fn test_get_head_bytes_from_s3() {
  //   // Tilt up the local S3 server...
  //   let (root, service) = setup_service().unwrap();

  //   let bucket = "asd";
  //   let key = "qwe";
  //   let content = "Hello World!";

  //   fs_write_object(root, bucket, key, content).unwrap();

  //   let mut req = Request::new(Body::empty());
  //   *req.method_mut() = Method::GET;
  //   *req.uri_mut() = format!("http://localhost:8014/{}/{}", bucket, key)
  //       .parse()
  //       .unwrap();
  //   req.headers_mut().insert(
  //       X_AMZ_CONTENT_SHA256.clone(),
  //       HeaderValue::from_static("UNSIGNED-PAYLOAD"),
  //   );

  //   let mut res = service.hyper_call(req).await.unwrap();
  //   let body = recv_body_string(&mut res).await.unwrap();

  //   // TODO: Find an aws_sdk_rust equivalent? Not sure this exists :_S
  //   // let local_region = Region::Custom {
  //   //   endpoint: "http://localhost:8014".to_owned(),
  //   //   name: "local".to_owned(),
  //   // };

  //   let s3_storage = AwsS3Storage::new(
  //     aws_s3_client().await,
  //     bucket.to_string(),
  //     key.to_string(),
  //   );

  //   let obj_head = format!("http://localhost:8014/{}/{}", bucket, key);
  //   //dbg!(&obj_head);
  //   let htsget_head = s3_storage.head(obj_head).await.unwrap();

  //   // assert_eq!(res.status(), StatusCode::OK);
  //   // assert_eq!(body, content);
  // }

  #[tokio::test]
  async fn test_get_local_s3_server_object() {
    let (root, service) = setup_service().unwrap();

    let bucket = "asd";
    let key = "qwe";
    let content = "Hello World!";

    fs_write_object(root, bucket, key, content).unwrap();

    let mut req = Request::new(Body::empty());
    *req.method_mut() = Method::GET;
    *req.uri_mut() = format!("http://localhost:8014/{}/{}", bucket, key)
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

  #[tokio::test]
  async fn local_s3_server_returns_htsget_url() {
    let (root, service) = setup_service().unwrap();

    let bucket = "bucket";
    let key = "key";
    let content = "Hello World!";
  }
}
