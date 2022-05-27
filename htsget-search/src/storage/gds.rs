/// ICAv1 and ICAv2
pub enum ICAVersion {
    v1,
    v2
}

/// Implementation for the [Storage] trait utilising data from an Illumina GDS storage server.
#[derive(Debug, Clone)]
pub struct IlluminaGDS {
  client: Client,
  volume: String, // TODO: Perhaps a Cargo feature instead? Would it make sense to target both versions from a single htsget server?
  version: ICAVersion,
  id_resolver: RegexResolver,
}

impl IlluminaGDS {
  pub fn new(client: Client, volume: String, ica_version: ICAVersion, id_resolver: RegexResolver) -> Self {
    IlluminaGDS {
      client,
      volume,
      ica_version,
      id_resolver,
    }
  }

  pub async fn new_with_default_config(volume: String, id_resolver: RegexResolver) -> Self {
    IlluminaGDS {
      //Client::new(&gds_config::load_from_env().await),
      volume,
      ica_version,
      id_resolver,
    }
  }

  fn resolve_path<K: AsRef<str> + Send>(&self, key: K) -> Result<String> {
    unimplemented!()
  }
}

impl Storage for IlluminaGDS {
  async fn get() -> Result<Self> {
    unimplemented!()
  }
  async fn url() -> Result<String> {
    unimplemented!()
  }
  async fn head() -> Result<()> {
    unimplemented!()
  }
}