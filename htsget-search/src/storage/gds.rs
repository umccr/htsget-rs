//! Module providing an implementation for the [Storage] trait using Illumina's ICA GDS object storage service.

use async_trait::async_trait;
use std::collections::HashMap;

// Reqwest
use reqwest::header::RANGE;
use reqwest::header::{HeaderMap, HeaderValue};

// GDS
use ica_gds::apis::configuration::Configuration;
use ica_gds::util::{presigned_url, setup_conf};

// Streamable
use bytes::Bytes;
use std::io::Cursor;
use tokio::io::BufReader;

// Htsget
use crate::htsget::Url;
use htsget_config::Query;
use htsget_config::regex_resolver::{Resolver, RegexResolver};

use super::{GetOptions, Result};
use crate::storage::{Headers, RangeUrlOptions, Storage, StorageError};

/// Implementation for the [Storage] trait utilising data from an Illumina ICA GDS storage server.
#[derive(Debug, Clone)]
pub struct GDSStorage {
  conf: Configuration,
  id_resolver: RegexResolver,
}

impl GDSStorage {
  pub async fn new(id_resolver: RegexResolver) -> Self {
    let conf = setup_conf().await.unwrap();
    GDSStorage {
      conf, // Stores auth data, client and endpoint. URLs (keys) don't go here but
      // provided to the Storage trait below, directly.
      id_resolver,
    }
  }

  async fn resolve_key<K: AsRef<str>>(&self, key: &K) -> Result<String> {
    self
      .id_resolver
      // FIXME: This must be wrong, a GDSv1 resolver doesn't work like this?
      .resolve_id(&Query::new("id", htsget_config::Format::Bam))
      .ok_or_else(|| StorageError::InvalidKey(key.as_ref().to_string()))
  }

  async fn get_content<K: AsRef<str> + Send>(&self, key: K, options: GetOptions) -> Result<Bytes> {
    let url = presigned_url(key.as_ref()).await?;
    let client = &self.conf.client;
    let mut headers = HeaderMap::new();
    // TODO: Hyper or Reqwest here?
    let range = HeaderValue::from_str(
      format!(
        "{}-{}",
        options.range.start.unwrap(),
        options.range.end.unwrap()
      )
      .as_str(),
    )
    .unwrap();
    headers.insert(RANGE, range);

    client
      .request(reqwest::Method::GET, url)
      .headers(headers)
      .send()
      .await?
      .bytes()
      .await
      .map_err(|e| StorageError::GDSRetrievalError(e))
  }

  async fn create_buf_reader<K: AsRef<str> + Send>(
    &self,
    key: K,
    options: GetOptions,
  ) -> Result<BufReader<Cursor<Bytes>>> {
    let response = self.get_content(key, options).await?;
    let cursor = Cursor::new(response);
    let reader = BufReader::new(cursor);
    Ok(reader)
  }

  async fn gds_presign_url<K: AsRef<str> + Send>(&self, key: K) -> Result<Url> {
    let resolved_key = self.resolve_key(&key).await?;
    let presigned = presigned_url(resolved_key.as_str());
    let htsget_url = Url::new(presigned.await?).await;
    Ok(htsget_url)
  }
}

#[async_trait]
impl Storage for GDSStorage {
  type Streamable = BufReader<Cursor<Bytes>>;

  async fn get<K: AsRef<str> + Send>(
    &self,
    key: K,
    options: GetOptions,
  ) -> Result<Self::Streamable> {
    let key = key.as_ref();
    //debug!(calling_from = self, key, "Getting GDS file from gds://{:?}", key);
    //let bufreader = self.create_buf_reader(key, options).await?;
    //let bytes = self.get_content(key, options);
    self.create_buf_reader(key, options).await
  }
  async fn range_url<K: AsRef<str> + Send>(&self, key: K, options: RangeUrlOptions) -> Result<Url> {
    let key = key.as_ref().to_owned();
    let hashmap = HashMap::new();
    let bytes_range = format!(
      "{}-{}",
      options.range.start.unwrap(),
      options.range.end.unwrap()
    );
    let headers = Headers::new(hashmap).with_header("Range".to_string(), bytes_range);
    let gds_presigned_url = self
      .gds_presign_url(key)
      .await?
      .with_headers(headers)
      .with_class(options.range.class);
    //debug!(calling_from = ?self, key, ?url, "Getting url with key {:?}", key);
    Ok(gds_presigned_url)
  }
  async fn head<K: AsRef<str> + Send>(&self, key: K) -> Result<u64> {
    let key = key.as_ref();
    let presigned = self.gds_presign_url(key).await?.url;
    Ok(
      self
        .conf
        .client
        .get(presigned)
        .send()
        .await?
        .content_length()
        .unwrap(),
    )
  }
}
