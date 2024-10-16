use std::str::FromStr;

use http::uri::Authority;
use serde::{Deserialize, Serialize};

use crate::config::{default_localstorage_addr, default_path, DataServerConfig};
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use crate::tls::KeyPairScheme;
use crate::types::Scheme;

pub(crate) fn default_authority() -> Authority {
  Authority::from_static(default_localstorage_addr())
}

fn default_local_path() -> String {
  default_path().into()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Local {
  scheme: Scheme,
  #[serde(with = "http_serde::authority")]
  authority: Authority,
  local_path: String,
  path_prefix: String,
  use_data_server_config: bool,
  #[serde(skip_serializing)]
  #[cfg(feature = "experimental")]
  keys: Option<C4GHKeys>,
}

impl Local {
  /// Create a new local storage.
  pub fn new(
    scheme: Scheme,
    authority: Authority,
    local_path: String,
    path_prefix: String,
    use_data_server_config: bool,
  ) -> Self {
    Self {
      scheme,
      authority,
      local_path,
      path_prefix,
      use_data_server_config,
      #[cfg(feature = "experimental")]
      keys: None,
    }
  }

  /// Get the scheme.
  pub fn scheme(&self) -> Scheme {
    self.scheme
  }

  /// Get the authority.
  pub fn authority(&self) -> &Authority {
    &self.authority
  }

  /// Get the local path.
  pub fn local_path(&self) -> &str {
    &self.local_path
  }

  /// Get the path prefix.
  pub fn path_prefix(&self) -> &str {
    &self.path_prefix
  }

  /// Get whether config should be inherited from the data server config.
  pub fn use_data_server_config(&self) -> bool {
    self.use_data_server_config
  }

  #[cfg(feature = "experimental")]
  /// Set the C4GH keys.
  pub fn set_keys(mut self, keys: Option<C4GHKeys>) -> Self {
    self.keys = keys;
    self
  }

  #[cfg(feature = "experimental")]
  /// Get the C4GH keys.
  pub fn keys(&self) -> Option<&C4GHKeys> {
    self.keys.as_ref()
  }
}

impl Default for Local {
  fn default() -> Self {
    Self::new(
      Scheme::Http,
      default_authority(),
      default_local_path(),
      Default::default(),
      false,
    )
  }
}

impl From<&DataServerConfig> for Local {
  fn from(config: &DataServerConfig) -> Self {
    Self::new(
      config.tls().get_scheme(),
      Authority::from_str(&config.addr().to_string()).expect("expected valid authority"),
      config.local_path().to_string_lossy().to_string(),
      config.serve_at().to_string(),
      true,
    )
  }
}

#[cfg(test)]
mod tests {
  use std::net::SocketAddr;
  use std::path::PathBuf;

  use crate::config::cors::CorsConfig;
  use crate::config::tests::test_config_from_file;
  use crate::storage::Storage;
  use crate::types::Scheme::Http;

  use super::*;

  #[test]
  fn config_storage_local_file() {
    test_config_from_file(
      r#"
        [[resolvers]]
        regex = "regex"

        [resolvers.storage]
        backend = "Local"
        local_path = "path"
        scheme = "HTTPS"
        path_prefix = "path"
        "#,
      |config| {
        println!("{:?}", config.resolvers().first().unwrap().storage());
        assert!(matches!(
            config.resolvers().first().unwrap().storage(),
            Storage::Local(local_storage) if local_storage.local_path() == "path" && local_storage.scheme() == Scheme::Https && local_storage.path_prefix() == "path"
        ));
      },
    );
  }

  #[test]
  fn local_storage_from_data_server_config() {
    let data_server_config = DataServerConfig::new(
      true,
      SocketAddr::from_str("127.0.0.1:8080").unwrap(),
      PathBuf::from("data"),
      "/data".to_string(),
      None,
      CorsConfig::default(),
    );
    let result: Local = (&data_server_config).into();
    let expected = Local::new(
      Http,
      Authority::from_static("127.0.0.1:8080"),
      "data".to_string(),
      "/data".to_string(),
      true,
    );

    assert_eq!(result.scheme(), expected.scheme());
    assert_eq!(result.authority(), expected.authority());
    assert_eq!(result.local_path(), expected.local_path());
    assert_eq!(result.path_prefix(), expected.path_prefix());
    assert_eq!(
      result.use_data_server_config(),
      expected.use_data_server_config()
    );
  }
}
