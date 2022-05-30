use ica_gds::apis::configuration::{Configuration};
//use ica_gds::apis::volumes_api::{get_volume};

// Streamable
use bytes::Bytes;
use tokio::io::BufReader;
use std::io::Cursor;

use async_trait::async_trait;

use htsget_config::regex_resolver::{RegexResolver};

use crate::htsget::Url;
use crate::storage::Storage;

use super::{GetOptions, Result, UrlOptions};

// /// ICAv1 and ICAv2
// pub enum ICAVersion {
//     V1,
//     V2
// }

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

  // TODO: Handle ~/.ica/.session.aps2.yaml
  // TODO: Handle env vars like the ICA JWT rotator
  pub async fn new_with_default_config(volume: String, id_resolver: RegexResolver) -> Self {
    GDSStorage::new(
      Configuration::default(),
      volume,
      id_resolver,
    )
  }

  // fn resolve_path<K: AsRef<str> + Send>(&self, key: K) -> Result<String> {
  //   unimplemented!()
  // }
}

#[async_trait]
impl Storage for GDSStorage {
  type Streamable = BufReader<Cursor<Bytes>>;

  async fn get<K: AsRef<str> + Send>(&self, key: K, options: GetOptions) -> Result<Self::Streamable> {
    //get_volume(client, volume_id, tenant_id, metadata_include, metadata_exclude);
    unimplemented!()
  }
  async fn url<K: AsRef<str> + Send>(&self, key: K, options: UrlOptions) -> Result<Url> {
    unimplemented!()
  }
  async fn head<K: AsRef<str> + Send>(&self, key: K) -> Result<u64> {
    unimplemented!()
  }
}