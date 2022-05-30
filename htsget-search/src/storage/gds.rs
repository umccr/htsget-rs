use ica_gds::apis::configuration::{Configuration};
use ica_gds::apis::volumes_api::{get_volume};
use crate::storage::Storage;
use async_trait::async_trait;

use htsget_config::regex_resolver::{RegexResolver};

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

  // pub async fn new_with_default_config(volume: String, id_resolver: RegexResolver) -> Self {
  //   GDSStorage {
  //     client,
  //     volume,
  //     id_resolver,
  //   }
  // }

  fn resolve_path<K: AsRef<str> + Send>(&self, key: K) -> Result<String> {
    unimplemented!()
  }
}

#[async_trait]
impl Storage for GDSStorage {
  async fn get() -> Result<V: AsRef<str> + Send> {
    unimplemented!()
  }
  async fn url() -> Result<Url> {
    unimplemented!()
  }
  async fn head() -> Result<u64> {
    unimplemented!()
  }
}