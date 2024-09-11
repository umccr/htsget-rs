//! Module providing an implementation for the [StorageTrait] trait using Amazon's S3 object storage service.
//!

use std::fmt::Debug;
use std::io;
use std::io::ErrorKind::Other;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::error::{DisplayErrorContext, SdkError};
use aws_sdk_s3::operation::get_object::builders::GetObjectFluentBuilder;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::head_object::{HeadObjectError, HeadObjectOutput};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::StorageClass;
use aws_sdk_s3::Client;
use bytes::Bytes;
use futures::Stream;
use pin_project_lite::pin_project;
use tokio_util::io::StreamReader;
use tracing::instrument;
use tracing::{debug, warn};

use crate::s3::Retrieval::{Delayed, Immediate};
use crate::StorageError::{AwsS3Error, IoError, KeyNotFound};
use crate::{BytesPosition, HeadOptions, StorageError};
use crate::{BytesRange, StorageTrait};
use crate::{Streamable, Url};

use super::{GetOptions, RangeUrlOptions, Result};

/// Represents data classes that can be retrieved immediately or after a delay.
/// Specifically, Glacier Flexible, Glacier Deep Archive, and Intelligent Tiering archive
/// tiers have delayed retrieval, unless they have been restored.
#[derive(Debug)]
pub enum Retrieval {
  Immediate(StorageClass),
  Delayed(StorageClass),
}

/// Implementation for the [StorageTrait] trait utilising data from an S3 bucket.
#[derive(Debug, Clone)]
pub struct S3Storage {
  client: Client,
  bucket: String,
}

impl S3Storage {
  // Allow the user to set this?
  pub const PRESIGNED_REQUEST_EXPIRY: u64 = 1000;

  pub fn new(client: Client, bucket: String) -> Self {
    S3Storage { client, bucket }
  }

  pub async fn new_with_default_config(
    bucket: String,
    endpoint: Option<String>,
    path_style: bool,
  ) -> Self {
    let sdk_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&sdk_config);
    s3_config_builder.set_endpoint_url(endpoint); // For local S3 storage, i.e: Minio
    s3_config_builder.set_force_path_style(Some(path_style));

    let client = s3_config_builder.build();
    let s3_client = Client::from_conf(client);

    S3Storage::new(s3_client, bucket)
  }

  /// Return an S3 pre-signed URL of the key. This function does not check that the key exists,
  /// so this should be checked before calling it.
  pub async fn s3_presign_url<K: AsRef<str> + Send>(
    &self,
    key: K,
    range: &BytesPosition,
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
        .map_err(|err| Self::map_get_error(key, err))?
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
      .map_err(|err| {
        warn!("S3 error: {}", DisplayErrorContext(&err));

        let err = err.into_service_error();
        if let HeadObjectError::NotFound(_) = err {
          KeyNotFound(key.as_ref().to_string())
        } else {
          AwsS3Error(err.to_string(), key.as_ref().to_string())
        }
      })
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

  fn apply_range(builder: GetObjectFluentBuilder, range: &BytesPosition) -> GetObjectFluentBuilder {
    let range: String = String::from(&BytesRange::from(range));
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
    options: GetOptions<'_>,
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
    let response = Self::apply_range(response, options.range());
    Ok(
      response
        .send()
        .await
        .map_err(|err| Self::map_get_error(key, err))?
        .body,
    )
  }

  async fn create_stream_reader<K: AsRef<str> + Send>(
    &self,
    key: K,
    options: GetOptions<'_>,
  ) -> Result<StreamReader<S3Stream, Bytes>> {
    Ok(StreamReader::new(S3Stream::new(
      self.get_content(key, options).await?,
    )))
  }

  fn map_get_error<K, T>(key: K, error: SdkError<GetObjectError, T>) -> StorageError
  where
    K: AsRef<str> + Send,
    T: Debug + Send + Sync + 'static,
  {
    warn!("S3 error: {}", DisplayErrorContext(&error));

    let error = error.into_service_error();
    if let GetObjectError::NoSuchKey(_) = error {
      KeyNotFound(key.as_ref().to_string())
    } else {
      AwsS3Error(error.to_string(), key.as_ref().to_string())
    }
  }
}

pin_project! {
  /// A wrapper around a `ByteStream` in order to implement `Stream`.
  pub struct S3Stream {
    #[pin]
    inner: ByteStream
  }
}

impl S3Stream {
  /// Create a new S3Stream.
  pub fn new(inner: ByteStream) -> Self {
    S3Stream { inner }
  }
}

impl Stream for S3Stream {
  type Item = Result<Bytes>;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self
      .project()
      .inner
      .poll_next(cx)
      .map_err(|err| IoError("io error".to_string(), err.into()))
  }
}

#[async_trait]
impl StorageTrait for S3Storage {
  /// Gets the actual s3 object as a buffered reader.
  #[instrument(level = "trace", skip(self))]
  async fn get(&self, key: &str, options: GetOptions<'_>) -> Result<Streamable> {
    debug!(calling_from = ?self, key, "getting file with key {:?}", key);

    Ok(Streamable::from_async_read(
      self.create_stream_reader(key, options).await?,
    ))
  }

  /// Return an S3 pre-signed htsget URL. This function does not check that the key exists, so this
  /// should be checked before calling it.
  #[instrument(level = "trace", skip(self))]
  async fn range_url(&self, key: &str, options: RangeUrlOptions<'_>) -> Result<Url> {
    let presigned_url = self.s3_presign_url(key, options.range()).await?;
    let url = options.apply(Url::new(presigned_url));

    debug!(calling_from = ?self, key, ?url, "getting url with key {:?}", key);
    Ok(url)
  }

  /// Returns the size of the S3 object in bytes.
  #[instrument(level = "trace", skip(self))]
  async fn head(&self, key: &str, _options: HeadOptions<'_>) -> Result<u64> {
    let head = self.s3_head(key).await?;

    let content_length = head
      .content_length()
      .ok_or_else(|| AwsS3Error("unknown content length".to_string(), key.to_string()))?;

    let len = u64::try_from(content_length).map_err(|err| {
      IoError(
        "failed to convert file length to `u64`".to_string(),
        io::Error::new(Other, err),
      )
    })?;

    debug!(calling_from = ?self, key, len, "size of key {:?} is {}", key, len);
    Ok(len)
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::future::Future;
  use std::path::Path;
  use std::sync::Arc;

  use htsget_test::aws_mocks::with_s3_test_server;

  use crate::local::tests::create_local_test_files;
  use crate::s3::S3Storage;
  use crate::Headers;
  use crate::{BytesPosition, GetOptions, RangeUrlOptions, StorageTrait};
  use crate::{HeadOptions, StorageError};

  pub(crate) async fn with_aws_s3_storage_fn<F, Fut>(test: F, folder_name: String, base_path: &Path)
  where
    F: FnOnce(Arc<S3Storage>) -> Fut,
    Fut: Future<Output = ()>,
  {
    with_s3_test_server(base_path, |client| async move {
      test(Arc::new(S3Storage::new(client, folder_name))).await;
    })
    .await;
  }

  async fn with_aws_s3_storage<F, Fut>(test: F)
  where
    F: FnOnce(Arc<S3Storage>) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (folder_name, base_path) = create_local_test_files().await;
    with_aws_s3_storage_fn(test, folder_name, base_path.path()).await;
  }

  #[tokio::test]
  async fn existing_key() {
    with_aws_s3_storage(|storage| async move {
      let result = storage
        .get(
          "key2",
          GetOptions::new_with_default_range(&Default::default()),
        )
        .await;
      assert!(result.is_ok());
    })
    .await;
  }

  #[tokio::test]
  async fn non_existing_key() {
    with_aws_s3_storage(|storage| async move {
      let result = storage
        .get(
          "non-existing-key",
          GetOptions::new_with_default_range(&Default::default()),
        )
        .await;
      assert!(matches!(result, Err(StorageError::AwsS3Error(_, _))));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_existing_key() {
    with_aws_s3_storage(|storage| async move {
      let result = storage
        .range_url(
          "key2",
          RangeUrlOptions::new_with_default_range(&Default::default()),
        )
        .await
        .unwrap();
      assert!(result.url.starts_with("http://folder.localhost:8014/key2"));
      assert!(result.url.contains(&format!(
        "Amz-Expires={}",
        S3Storage::PRESIGNED_REQUEST_EXPIRY
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
          RangeUrlOptions::new(
            BytesPosition::new(Some(7), Some(9), None),
            &Default::default(),
          ),
        )
        .await
        .unwrap();
      assert!(result.url.starts_with("http://folder.localhost:8014/key2"));
      assert!(result.url.contains(&format!(
        "Amz-Expires={}",
        S3Storage::PRESIGNED_REQUEST_EXPIRY
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
          RangeUrlOptions::new(BytesPosition::new(Some(7), None, None), &Default::default()),
        )
        .await
        .unwrap();
      assert!(result.url.starts_with("http://folder.localhost:8014/key2"));
      assert!(result.url.contains(&format!(
        "Amz-Expires={}",
        S3Storage::PRESIGNED_REQUEST_EXPIRY
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
      let result = storage
        .head("key2", HeadOptions::new(&Default::default()))
        .await;
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
