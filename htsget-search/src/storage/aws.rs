//! Module providing an implementation for the [Storage] trait using Amazon's S3 object storage service.
use std::fmt::Debug;
use std::io;
use std::io::ErrorKind::Other;
use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::client::fluent_builders;
use aws_sdk_s3::error::GetObjectErrorKind;
use aws_sdk_s3::model::StorageClass;
use aws_sdk_s3::output::HeadObjectOutput;
use aws_sdk_s3::presigning::config::PresigningConfig;
use aws_sdk_s3::types::ByteStream;
use aws_sdk_s3::Client;
use bytes::Bytes;
use fluent_builders::GetObject;
use tokio_util::io::StreamReader;
use tracing::debug;
use tracing::instrument;

use crate::htsget::Url;
use crate::storage::aws::Retrieval::{Delayed, Immediate};
use crate::storage::StorageError::AwsS3Error;
use crate::storage::{BytesPosition, StorageError};
use crate::storage::{BytesRange, Storage};

use super::{GetOptions, RangeUrlOptions, Result};

/// Represents data classes that can be retrieved immediately or after a delay.
/// Specifically, Glacier Flexible, Glacier Deep Archive, and Intelligent Tiering archive
/// tiers have delayed retrieval, unless they have been restored.
#[derive(Debug)]
pub enum Retrieval {
  Immediate(StorageClass),
  Delayed(StorageClass),
}

/// Implementation for the [Storage] trait utilising data from an S3 bucket.
#[derive(Debug, Clone)]
pub struct AwsS3Storage {
  client: Client,
  bucket: String,
}

impl AwsS3Storage {
  // Allow the user to set this?
  pub const PRESIGNED_REQUEST_EXPIRY: u64 = 1000;

  pub fn new(client: Client, bucket: String) -> Self {
    AwsS3Storage { client, bucket }
  }

  pub async fn new_with_default_config(bucket: String) -> Self {
    AwsS3Storage::new(Client::new(&aws_config::load_from_env().await), bucket)
  }

  /// Return an S3 pre-signed URL of the key. This function does not check that the key exists,
  /// so this should be checked before calling it.
  pub async fn s3_presign_url<K: AsRef<str> + Send>(
    &self,
    key: K,
    range: BytesPosition,
  ) -> Result<String> {
    let response = self
      .client
      .get_object()
      .bucket(&self.bucket)
      .key(key.as_ref());
    let response = Self::apply_range(response, range);
    Ok(
      response
        .presigned(
          PresigningConfig::expires_in(Duration::from_secs(Self::PRESIGNED_REQUEST_EXPIRY))
            .map_err(|err| AwsS3Error(err.to_string(), key.as_ref().to_string()))?,
        )
        .await
        .map_err(|err| {
          let err = err.into_service_error();
          if let GetObjectErrorKind::NoSuchKey(_) = err.kind {
            StorageError::KeyNotFound(key.as_ref().to_string())
          } else {
            AwsS3Error(err.to_string(), key.as_ref().to_string())
          }
        })?
        .uri()
        .to_string(),
    )
  }

  async fn s3_head<K: AsRef<str> + Send>(&self, key: K) -> Result<HeadObjectOutput> {
    self
      .client
      .head_object()
      .bucket(&self.bucket)
      .key(key.as_ref())
      .send()
      .await
      .map_err(|err| AwsS3Error(err.to_string(), key.as_ref().to_string()))
  }

  /// Returns the retrieval type of the object stored with the key.
  #[instrument(level = "trace", skip_all, ret)]
  pub async fn get_retrieval_type<K: AsRef<str> + Send>(&self, key: K) -> Result<Retrieval> {
    let head = self.s3_head(key.as_ref()).await?;
    Ok(
      // Default is Standard.
      match head.storage_class.unwrap_or(StorageClass::Standard) {
        class @ (StorageClass::DeepArchive | StorageClass::Glacier) => {
          Self::check_restore_header(head.restore, class)
        }
        class @ StorageClass::IntelligentTiering => {
          if head.archive_status.is_some() {
            // Not sure if this check is necessary for the archived intelligent tiering classes but
            // it shouldn't hurt.
            Self::check_restore_header(head.restore, class)
          } else {
            Immediate(class)
          }
        }
        class => Immediate(class),
      },
    )
  }

  fn check_restore_header(restore_header: Option<String>, class: StorageClass) -> Retrieval {
    if let Some(restore) = restore_header {
      if restore.contains("ongoing-request=\"false\"") {
        return Immediate(class);
      }
    }
    Delayed(class)
  }

  fn apply_range(builder: GetObject, range: BytesPosition) -> GetObject {
    let range: String = String::from(&BytesRange::from(&range));
    if range.is_empty() {
      builder
    } else {
      builder.range(range)
    }
  }

  /// Get the key from S3 storage as a `ByteStream`.
  pub async fn get_content<K: AsRef<str> + Send>(
    &self,
    key: K,
    options: GetOptions,
  ) -> Result<ByteStream> {
    if let Delayed(class) = self.get_retrieval_type(key.as_ref()).await? {
      return Err(AwsS3Error(
        format!("cannot retrieve object immediately, class is `{class:?}`"),
        key.as_ref().to_string(),
      ));
    }

    let response = self
      .client
      .get_object()
      .bucket(&self.bucket)
      .key(key.as_ref());
    let response = Self::apply_range(response, options.range);
    Ok(
      response
        .send()
        .await
        .map_err(|err| AwsS3Error(err.to_string(), key.as_ref().to_string()))?
        .body,
    )
  }

  async fn create_stream_reader<K: AsRef<str> + Send>(
    &self,
    key: K,
    options: GetOptions,
  ) -> Result<StreamReader<ByteStream, Bytes>> {
    let response = self.get_content(key, options).await?;
    Ok(StreamReader::new(response))
  }
}

#[async_trait]
impl Storage for AwsS3Storage {
  type Streamable = StreamReader<ByteStream, Bytes>;

  /// Gets the actual s3 object as a buffered reader.
  #[instrument(level = "trace", skip(self))]
  async fn get<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: GetOptions,
  ) -> Result<Self::Streamable> {
    let key = key.as_ref();
    debug!(calling_from = ?self, key, "getting file with key {:?}", key);

    self.create_stream_reader(key, options).await
  }

  /// Return an S3 pre-signed htsget URL. This function does not check that the key exists, so this
  /// should be checked before calling it.
  #[instrument(level = "trace", skip(self))]
  async fn range_url<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: RangeUrlOptions,
  ) -> Result<Url> {
    let key = key.as_ref();
    let presigned_url = self.s3_presign_url(key, options.range.clone()).await?;
    let url = options.apply(Url::new(presigned_url));

    debug!(calling_from = ?self, key, ?url, "getting url with key {:?}", key);
    Ok(url)
  }

  /// Returns the size of the S3 object in bytes.
  #[instrument(level = "trace", skip(self))]
  async fn head<K: AsRef<str> + Send + Debug>(&self, key: K) -> Result<u64> {
    let key = key.as_ref();

    let head = self.s3_head(key).await?;
    let len = u64::try_from(head.content_length).map_err(|err| {
      StorageError::IoError(
        "failed to convert file length to `u64`".to_string(),
        io::Error::new(Other, err),
      )
    })?;

    debug!(calling_from = ?self, key, len, "size of key {:?} is {}", key, len);
    Ok(len)
  }
}

#[cfg(test)]
mod tests {
  use std::future::Future;
  use std::path::Path;

  use aws_config::SdkConfig;
  use aws_credential_types::provider::SharedCredentialsProvider;
  use aws_sdk_s3::{Client, Credentials, Region};
  use s3s::service::S3Service;
  use s3s_aws;

  use crate::htsget::Headers;
  use crate::storage::aws::AwsS3Storage;
  use crate::storage::local::tests::create_local_test_files;
  use crate::storage::StorageError;
  use crate::storage::{BytesPosition, GetOptions, RangeUrlOptions, Storage};

  async fn with_s3_test_server<F, Fut>(server_base_path: &Path, test: F)
  where
    F: FnOnce(Client) -> Fut,
    Fut: Future<Output = ()>,
  {
    const DOMAIN_NAME: &str = "localhost:8014";
    const REGION: &str = "ap-southeast-2";

    let cred = Credentials::for_tests();

    let conn = {
      let fs = s3s_fs::FileSystem::new(server_base_path).unwrap();

      let auth = s3s::SimpleAuth::from_single(cred.access_key_id(), cred.secret_access_key());

      let mut service = S3Service::new(Box::new(fs));
      service.set_auth(Box::new(auth));
      service.set_base_domain(DOMAIN_NAME);

      s3s_aws::Connector::from(service.into_shared())
    };

    let sdk_config = SdkConfig::builder()
      .credentials_provider(SharedCredentialsProvider::new(cred))
      .http_connector(conn)
      .region(Region::new(REGION))
      .endpoint_url(format!("http://{DOMAIN_NAME}"))
      .build();

    test(Client::new(&sdk_config)).await;
  }

  async fn with_aws_s3_storage<F, Fut>(test: F)
  where
    F: FnOnce(AwsS3Storage) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (folder_name, base_path) = create_local_test_files().await;
    with_s3_test_server(base_path.path(), |client| async move {
      test(AwsS3Storage::new(client, folder_name)).await;
    })
    .await;
  }

  #[tokio::test]
  async fn existing_key() {
    with_aws_s3_storage(|storage| async move {
      let result = storage.get("key2", GetOptions::default()).await;
      assert!(matches!(result, Ok(_)));
    })
    .await;
  }

  #[tokio::test]
  async fn non_existing_key() {
    with_aws_s3_storage(|storage| async move {
      let result = storage.get("non-existing-key", GetOptions::default()).await;
      assert!(matches!(result, Err(StorageError::AwsS3Error(_, _))));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_existing_key() {
    with_aws_s3_storage(|storage| async move {
      let result = storage
        .range_url("key2", RangeUrlOptions::default())
        .await
        .unwrap();
      assert!(result.url.starts_with("http://folder.localhost:8014/key2"));
      assert!(result.url.contains(&format!(
        "Amz-Expires={}",
        AwsS3Storage::PRESIGNED_REQUEST_EXPIRY
      )));
    })
    .await;
  }

  #[tokio::test]
  async fn url_with_specified_range() {
    with_aws_s3_storage(|storage| async move {
      let result = storage
        .range_url(
          "key2",
          RangeUrlOptions::default().with_range(BytesPosition::new(Some(7), Some(9), None)),
        )
        .await
        .unwrap();
      assert!(result.url.starts_with("http://folder.localhost:8014/key2"));
      assert!(result.url.contains(&format!(
        "Amz-Expires={}",
        AwsS3Storage::PRESIGNED_REQUEST_EXPIRY
      )));
      assert!(result.url.contains("range"));
      assert_eq!(
        result.headers,
        Some(Headers::default().with_header("Range", "bytes=7-8"))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn url_with_specified_open_ended_range() {
    with_aws_s3_storage(|storage| async move {
      let result = storage
        .range_url(
          "key2",
          RangeUrlOptions::default().with_range(BytesPosition::new(Some(7), None, None)),
        )
        .await
        .unwrap();
      assert!(result.url.starts_with("http://folder.localhost:8014/key2"));
      assert!(result.url.contains(&format!(
        "Amz-Expires={}",
        AwsS3Storage::PRESIGNED_REQUEST_EXPIRY
      )));
      assert!(result.url.contains("range"));
      assert_eq!(
        result.headers,
        Some(Headers::default().with_header("Range", "bytes=7-"))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn file_size() {
    with_aws_s3_storage(|storage| async move {
      let result = storage.head("key2").await;
      let expected: u64 = 6;
      assert!(matches!(result, Ok(size) if size == expected));
    })
    .await;
  }

  #[tokio::test]
  async fn retrieval_type() {
    with_aws_s3_storage(|storage| async move {
      let result = storage.get_retrieval_type("key2").await;
      println!("{result:?}");
    })
    .await;
  }
}
