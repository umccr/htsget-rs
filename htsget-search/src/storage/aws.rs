//! Module providing an implementation for the [Storage] trait using Amazon's S3 object storage service.
use regex::Regex;
use std::path::{ PathBuf };
use std::time::Duration;

use async_trait::async_trait;

use crate::htsget::Url;

use super::{GetOptions, Result, UrlOptions};
use crate::storage::async_storage::AsyncStorage;
use crate::storage::StorageError::InvalidKey;

use aws_config;

use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::input::GetObjectInput;
use aws_sdk_s3::presigning::config::PresigningConfig;
use aws_sdk_s3::{Client, Config, Region};

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

/// Implementation for the [Storage] trait using the local file system.
pub struct AwsS3Storage {
  client: Client,
  bucket: String,
  key: String,
}

impl AwsS3Storage {
  fn new(client: Client, bucket: String, key: String) -> Self {
    AwsS3Storage {
      client,
      bucket,
      key,
    }
  }

  // TODO: Take into account all S3 URL styles...: https://gist.github.com/bh1428/c30b7db493828ece622a6cb38c05362a
  async fn get_bucket_and_key_from_s3_url(s3_url: String) -> Result<(String, String)> {
    // TODO: Rewrite as match
    // match s3_url {
    //  
    // }
    if s3_url.starts_with("s3://") {
      let re = Regex::new(r"s3://([^/]+)/(.*)").unwrap();
      let cap = re.captures(&s3_url).unwrap();
      let bucket = cap[1].to_string();
      let key = cap[2].to_string();

      Ok((bucket, key))
    } else if s3_url.starts_with("http://") { // useful for local testing, not for prod
      let re = Regex::new(r"http://([^/]+)/(.*)").unwrap();
      let cap = re.captures(&s3_url).unwrap();
      let bucket = cap[1].to_string();
      let key = cap[2].to_string();

      Ok((bucket, key))
    } else if s3_url.starts_with("https://") {
      Err(InvalidKey(s3_url))
    } else {
      Err(InvalidKey(s3_url))
    }
  }

  async fn s3_presign_url(client: Client, bucket: String, key: String) -> Result<String> {
    let expires_in = Duration::from_secs(900);

    let region_provider = RegionProviderChain::first_try("ap-southeast-2")
    .or_default_provider()
    .or_else(Region::new("us-east-1"));
  
    let shared_config = aws_config::from_env().region(region_provider).load().await;

    // Presigned requests can be made with the client directly
    let presigned_request = client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .presigned(PresigningConfig::expires_in(expires_in).unwrap())
        .await;

    // Or, they can be made directly from an operation input
    let presigned_request = GetObjectInput::builder()
        .bucket(bucket)
        .key(key)
        .build().unwrap()
        .presigned(
            &Config::from(&shared_config),
            PresigningConfig::expires_in(expires_in).unwrap(),
        )
        .await;
        
    Ok(presigned_request.unwrap().uri().to_string())
  }

  async fn s3_head(client: Client, bucket: String, key: String) -> Result<u64> {
    let content_length = client
        .head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .unwrap()
        .content_length as u64;

    dbg!(content_length);
    Ok(content_length)
  }

  async fn get_storage_tier(s3_url: String) -> Result<AwsS3StorageTier> {
    // 1. S3 head request to object
    // 2. Return status
    // Similar (Java) code I wrote here: https://github.com/igvteam/igv/blob/master/src/main/java/org/broad/igv/util/AmazonUtils.java#L257
    // Or with AWS cli with: $ aws s3api head-object --bucket awsexamplebucket --key dir1/example.obj
    unimplemented!();
  }
}

#[async_trait]
impl AsyncStorage for AwsS3Storage {
  /// Returns the S3 url (s3://bucket/key) for the given path (key).
  async fn get<K: AsRef<str> + Send>(&self, key: K, _options: GetOptions) -> Result<PathBuf> {
    let key: &str = key.as_ref();
    let (bucket, s3key) = AwsS3Storage::get_bucket_and_key_from_s3_url(key.to_string()).await?;

    let s3path = PathBuf::from(bucket).join(s3key);

    Ok(s3path)
  }

  /// Returns a S3-presigned htsget URL
  async fn url<K: AsRef<str> + Send>(&self, key: K, options: UrlOptions) -> Result<Url> {
    let region_provider = RegionProviderChain::first_try("ap-southeast-2")
      .or_default_provider()
      .or_else(Region::new("us-east-1"));
  
    let shared_config = aws_config::from_env().region(region_provider).load().await;
    
    let client = Client::new(&shared_config);

    let presigned_url =
      AwsS3Storage::s3_presign_url(client, self.bucket.clone(), key.as_ref().to_string());
    let htsget_url = Url::new(presigned_url.await?);
    Ok(htsget_url)
  }

  /// Returns the size of the S3 object in bytes.
  async fn head<K: AsRef<str> + Send>(&self, key: K) -> Result<u64> {
    let region_provider = RegionProviderChain::first_try("ap-southeast-2")
    .or_default_provider()
    .or_else(Region::new("us-east-1"));
  
    let shared_config = aws_config::from_env().region(region_provider).load().await;

    let key: &str = key.as_ref(); // input URI or path, not S3 key... the trait naming is a bit misleading
    let client = Client::new(&shared_config);

    let (bucket, s3key) = AwsS3Storage::get_bucket_and_key_from_s3_url(key.to_string()).await?;

    let object_bytes = AwsS3Storage::s3_head(client, self.bucket.clone(), s3key).await?;
    Ok(object_bytes)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::storage::s3_testing::setup_service;
  use crate::storage::s3_testing::recv_body_string;
  use crate::storage::s3_testing::fs_write_object;

  use hyper::{Body, Method, StatusCode};
  use s3_server::headers::HeaderValue;
  use s3_server::headers::X_AMZ_CONTENT_SHA256;

  type Request = hyper::Request<hyper::Body>;

  async fn aws_s3_client() -> Client {
    let region_provider = RegionProviderChain::first_try("ap-southeast-2")
    .or_default_provider()
    .or_else(Region::new("us-east-1"));

    let shared_config = aws_config::from_env().region(region_provider).load().await;
  
    Client::new(&shared_config)
  }

  #[tokio::test]
  async fn test_split_s3_url_into_bucket_and_key() {
    let s3_url = "s3://bucket/key";

    let (bucket, key) = AwsS3Storage::get_bucket_and_key_from_s3_url(s3_url.to_string())
      .await
      .unwrap();
      
    let s3_storage = AwsS3Storage::new(
      aws_s3_client().await,
      bucket.clone(),
      key.clone(),
    );

    assert_eq!(bucket, "bucket");
    assert_eq!(key, "key");
  }

  #[tokio::test]
  async fn test_get_htsget_url_from_s3() {
    let s3_url = "s3://bucket/key";

    let (bucket, key) = AwsS3Storage::get_bucket_and_key_from_s3_url(s3_url.to_string())
      .await
      .unwrap();

    let s3_storage = AwsS3Storage::new(
      aws_s3_client().await,
      bucket.clone(),
      key.clone(),
    );

    let htsget_url = s3_storage.url(key, UrlOptions::default()).await.unwrap();
    //dbg!(&htsget_url);
    assert!(htsget_url.url.contains("X-Amz-Signature"));
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