use async_trait::async_trait;
use tracing::debug;

// GDS
use ica_gds::apis::configuration::Configuration;
use ica_gds::util::{presigned_url, setup_conf};

// Streamable
use bytes::Bytes;
use std::io::Cursor;
use tokio::io::BufReader;

// Htsget
use crate::htsget::Url;
use htsget_config::regex_resolver::{RegexResolver, HtsGetIdResolver};

use super::{GetOptions, Result};
use crate::storage::{Storage, RangeUrlOptions, StorageError};

/// Implementation for the [Storage] trait utilising data from an Illumina GDS storage server.
#[derive(Debug, Clone)]
pub struct GDSStorage {
  client: Configuration,
  volume: String, // TODO: Perhaps a Cargo feature instead? Would it make sense to target both versions from a single htsget server?
  id_resolver: RegexResolver,
}

impl GDSStorage {
  pub fn new(client: Configuration, volume: String, id_resolver: RegexResolver) -> Self {
    GDSStorage {
      client,
      volume,
      id_resolver,
    }
  }
  pub async fn new_with_default_config(volume: String, id_resolver: RegexResolver) -> Self {
    GDSStorage::new(setup_conf().await, volume, id_resolver)
  }

  async fn resolve_key<K: AsRef<str>>(&self, key: &K) -> Result<String> {
    self
      .id_resolver
      .resolve_id(key.as_ref())
      .ok_or_else(|| StorageError::InvalidKey(key.as_ref().to_string()))
  }

  // fn apply_range(builder: GetObject, range: BytesRange) -> GetObject {
  //   // let range: String = range.into();
  //   // if range.is_empty() {
  //   //   builder
  //   // } else {
  //   //   builder.range(range)
  //   // }
  //   unimplemented!()
  // }

  async fn get_content<K: AsRef<str> + Send>(&self, key: K, _options: GetOptions) -> Result<Bytes> {
    let conf = setup_conf().await;
    let url = presigned_url(key.as_ref()).await?;
    Ok(conf.client.get(url).send().await?.bytes().await?)
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
    debug!(calling_from = ?self, key, "Getting file with key {:?}", key);
    self.create_buf_reader(key, options).await
  }
  async fn range_url<K: AsRef<str> + Send>(&self, key: K, _options: RangeUrlOptions) -> Result<Url> {
    let foo = key.as_ref().to_owned();
    self.gds_presign_url(foo).await
  }
  async fn head<K: AsRef<str> + Send>(&self, key: K) -> Result<u64> {
    let conf = setup_conf().await;
    let key = key.as_ref();
    let presigned = self.gds_presign_url(key).await?.url;
    Ok(
      conf
        .client
        .get(presigned)
        .send()
        .await?
        .content_length()
        .unwrap(),
    )
  }
}
