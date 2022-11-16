//! Module providing an implementation for the [Storage] trait using Amazon's S3 object storage service.
use std::fmt::Debug;
use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::client::fluent_builders;
use aws_sdk_s3::model::StorageClass;
use aws_sdk_s3::output::HeadObjectOutput;
use aws_sdk_s3::presigning::config::PresigningConfig;
use aws_sdk_s3::types::ByteStream;
use aws_sdk_s3::Client;
use bytes::Bytes;
use fluent_builders::GetObject;
use htsget_config::Query;
use tokio_util::io::StreamReader;
use tracing::debug;
use tracing::instrument;

use crate::htsget::Url;
use crate::storage::aws::Retrieval::{Delayed, Immediate};
use crate::storage::StorageError::AwsS3Error;
use crate::storage::{resolve_id, BytesPosition};
use crate::storage::{BytesRange, Storage};
use crate::RegexResolver;

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
  id_resolver: RegexResolver,
}

impl AwsS3Storage {
  // Allow the user to set this?
  pub const PRESIGNED_REQUEST_EXPIRY: u64 = 1000;

  pub fn new(client: Client, bucket: String, id_resolver: RegexResolver) -> Self {
    AwsS3Storage {
      client,
      bucket,
      id_resolver,
    }
  }

  pub async fn new_with_default_config(bucket: String, id_resolver: RegexResolver) -> Self {
    AwsS3Storage::new(
      Client::new(&aws_config::load_from_env().await),
      bucket,
      id_resolver,
    )
  }

  pub async fn s3_presign_url(&self, query: &Query, range: BytesPosition) -> Result<String> {
    let response = self
      .client
      .get_object()
      .bucket(&self.bucket)
      .key(resolve_id(&self.id_resolver, query)?);
    let response = Self::apply_range(response, range);
    Ok(
      response
        .presigned(
          PresigningConfig::expires_in(Duration::from_secs(Self::PRESIGNED_REQUEST_EXPIRY))
            .map_err(|err| AwsS3Error(err.to_string(), query.id.to_string()))?,
        )
        .await
        .map_err(|err| AwsS3Error(err.to_string(), query.id.to_string()))?
        .uri()
        .to_string(),
    )
  }

  async fn s3_head(&self, query: &Query) -> Result<HeadObjectOutput> {
    self
      .client
      .head_object()
      .bucket(&self.bucket)
      .key(resolve_id(&self.id_resolver, query)?)
      .send()
      .await
      .map_err(|err| AwsS3Error(err.to_string(), query.id.to_string()))
  }

  /// Returns the retrieval type of the object stored with the key.
  #[instrument(level = "trace", skip_all, ret)]
  pub async fn get_retrieval_type(&self, query: &Query) -> Result<Retrieval> {
    let head = self.s3_head(query).await?;
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

  pub async fn get_content(&self, query: &Query, options: GetOptions) -> Result<ByteStream> {
    if let Delayed(class) = self.get_retrieval_type(query).await? {
      return Err(AwsS3Error(
        format!("cannot retrieve object immediately, class is `{:?}`", class),
        query.id.to_string(),
      ));
    }

    let response = self
      .client
      .get_object()
      .bucket(&self.bucket)
      .key(resolve_id(&self.id_resolver, query)?);
    let response = Self::apply_range(response, options.range);
    Ok(
      response
        .send()
        .await
        .map_err(|err| AwsS3Error(err.to_string(), query.id.to_string()))?
        .body,
    )
  }

  async fn create_stream_reader(
    &self,
    query: &Query,
    options: GetOptions,
  ) -> Result<StreamReader<ByteStream, Bytes>> {
    let response = self.get_content(query, options).await?;
    Ok(StreamReader::new(response))
  }
}

#[async_trait]
impl Storage for AwsS3Storage {
  type Streamable = StreamReader<ByteStream, Bytes>;

  /// Gets the actual s3 object as a buffered reader.
  #[instrument(level = "trace", skip(self))]
  async fn get(&self, query: &Query, options: GetOptions) -> Result<Self::Streamable> {
    debug!(calling_from = ?self, query.id, "getting file with key {:?}", query.id);

    self.create_stream_reader(query, options).await
  }

  /// Returns a S3-presigned htsget URL
  #[instrument(level = "trace", skip(self))]
  async fn range_url(&self, query: &Query, options: RangeUrlOptions) -> Result<Url> {
    let presigned_url = self.s3_presign_url(query, options.range.clone()).await?;
    let url = options.apply(Url::new(presigned_url));

    debug!(calling_from = ?self, query.id, ?url, "getting url with key {:?}", query.id);
    Ok(url)
  }

  /// Returns the size of the S3 object in bytes.
  #[instrument(level = "trace", skip(self))]
  async fn head(&self, query: &Query) -> Result<u64> {
    let head = self.s3_head(query).await?;
    let len = head.content_length as u64; // Todo fix this for safe casting

    debug!(calling_from = ?self, query.id, len, "size of key {:?} is {}", query.id, len);
    Ok(len)
  }
}

#[cfg(test)]
mod tests {
  use std::future::Future;
  use std::net::TcpListener;
  use std::path::Path;

  use aws_sdk_s3::{Client, Endpoint};
  use aws_types::credentials::SharedCredentialsProvider;
  use aws_types::region::Region;
  use aws_types::{Credentials, SdkConfig};
  use futures::future;
  use htsget_config::regex_resolver::MatchOnQuery;
  use htsget_config::Format::Bam;
  use htsget_config::Query;
  use hyper::service::make_service_fn;
  use hyper::Server;
  use s3_server::storages::fs::FileSystem;
  use s3_server::{S3Service, SimpleAuth};

  use crate::htsget::Headers;
  use crate::storage::aws::AwsS3Storage;
  use crate::storage::local::tests::create_local_test_files;
  use crate::storage::StorageError;
  use crate::storage::{BytesPosition, GetOptions, RangeUrlOptions, Storage};
  use crate::RegexResolver;

  async fn with_s3_test_server<F, Fut>(server_base_path: &Path, test: F)
  where
    F: FnOnce(Client) -> Fut,
    Fut: Future<Output = ()>,
  {
    // Setup s3-server.
    let fs = FileSystem::new(server_base_path).unwrap();
    let mut auth = SimpleAuth::new();
    auth.register(String::from("access_key"), String::from("secret_key"));
    let mut service = S3Service::new(fs);
    service.set_auth(auth);

    // Spawn hyper Server instance.
    let service = service.into_shared();
    let listener = TcpListener::bind(("localhost", 0)).unwrap();
    let bound_addr = format!("http://localhost:{}", listener.local_addr().unwrap().port());
    let make_service: _ =
      make_service_fn(move |_| future::ready(Ok::<_, anyhow::Error>(service.clone())));
    tokio::spawn(Server::from_tcp(listener).unwrap().serve(make_service));

    // Create S3Config.
    let config = SdkConfig::builder()
      .region(Region::new("ap-southeast-2"))
      .credentials_provider(SharedCredentialsProvider::new(Credentials::from_keys(
        "access_key",
        "secret_key",
        None,
      )))
      .build();
    let ep = Endpoint::immutable(bound_addr.parse().unwrap());
    let s3_conf = aws_sdk_s3::config::Builder::from(&config)
      .endpoint_resolver(ep)
      .build();

    test(Client::from_conf(s3_conf));
  }

  async fn with_aws_s3_storage<F, Fut>(test: F)
  where
    F: FnOnce(AwsS3Storage) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (folder_name, base_path) = create_local_test_files().await;
    with_s3_test_server(base_path.path(), |client| async move {
      test(AwsS3Storage::new(
        client,
        folder_name,
        RegexResolver::new(".*", "$0", MatchOnQuery::default()).unwrap(),
      ));
    })
    .await;
  }

  #[tokio::test]
  async fn existing_key() {
    with_aws_s3_storage(|storage| async move {
      let result = storage
        .get(&Query::new("key2", Bam), GetOptions::default())
        .await;
      assert!(matches!(result, Ok(_)));
    })
    .await;
  }

  #[tokio::test]
  async fn non_existing_key() {
    with_aws_s3_storage(|storage| async move {
      let result = storage
        .get(&Query::new("non-existing-key", Bam), GetOptions::default())
        .await;
      assert!(matches!(result, Err(StorageError::AwsS3Error(_, _))));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_non_existing_key() {
    with_aws_s3_storage(|storage| async move {
      let result = storage
        .range_url(
          &Query::new("non-existing-key", Bam),
          RangeUrlOptions::default(),
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
        .range_url(&Query::new("key2", Bam), RangeUrlOptions::default())
        .await
        .unwrap();
      assert!(result
        .url
        .starts_with(&format!("http://localhost:8014/{}/{}", "folder", "key2")));
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
          &Query::new("key2", Bam),
          RangeUrlOptions::default().with_range(BytesPosition::new(Some(7), Some(9), None)),
        )
        .await
        .unwrap();
      assert!(result
        .url
        .starts_with(&format!("http://localhost:8014/{}/{}", "folder", "key2")));
      assert!(result.url.contains(&format!(
        "Amz-Expires={}",
        AwsS3Storage::PRESIGNED_REQUEST_EXPIRY
      )));
      assert!(result.url.contains("range"));
      assert_eq!(
        result.headers,
        Some(Headers::default().with_header("Range", "bytes=7-9"))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn url_with_specified_open_ended_range() {
    with_aws_s3_storage(|storage| async move {
      let result = storage
        .range_url(
          &Query::new("key2", Bam),
          RangeUrlOptions::default().with_range(BytesPosition::new(Some(7), None, None)),
        )
        .await
        .unwrap();
      assert!(result
        .url
        .starts_with(&format!("http://localhost:8014/{}/{}", "folder", "key2")));
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
      let result = storage.head(&Query::new("key2", Bam)).await;
      let expected: u64 = 6;
      assert!(matches!(result, Ok(size) if size == expected));
    })
    .await;
  }

  #[tokio::test]
  async fn retrieval_type() {
    with_aws_s3_storage(|storage| async move {
      let result = storage.get_retrieval_type(&Query::new("key2", Bam)).await;
      println!("{:?}", result);
    })
    .await;
  }
}
