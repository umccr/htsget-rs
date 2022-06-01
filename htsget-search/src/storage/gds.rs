use tracing::debug;

// GDS
use ica_gds::util::{ setup_conf, get_presigned_url };
use ica_gds::apis::configuration::Configuration;

// Streamable
use bytes::Bytes;
use std::io::Cursor;
use tokio::io::BufReader;

use async_trait::async_trait;

use htsget_config::regex_resolver::HtsGetIdResolver;
use htsget_config::regex_resolver::RegexResolver;

use crate::htsget::Url;
use crate::storage::Storage;
use crate::storage::{BytesRange, StorageError};

use super::{GetOptions, Result, UrlOptions};

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
    GDSStorage::new(
      setup_conf().await,
      volume,
      id_resolver,
    )
  }

  fn resolve_key<K: AsRef<str> + Send>(&self, key: &K) -> Result<String> {
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

  async fn get_content<K: AsRef<str> + Send>(&self, key: K, options: GetOptions) -> Result<Bytes> {
    // What about just getting the contents out of the already presigned URL? Via hyper/reqwest?
    // let conf = self.new_with_default_config().await;
    // get_presigned_url()
    unimplemented!()
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
}
  // fn resolve_path<K: AsRef<str> + Send>(&self, key: K) -> Result<String> {
  //   unimplemented!()
  // }


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
  async fn url<K: AsRef<str> + Send>(&self, key: K, options: UrlOptions) -> Result<Url> {
    unimplemented!()
  }
  async fn head<K: AsRef<str> + Send>(&self, key: K) -> Result<u64> {
    unimplemented!()
  }
}
