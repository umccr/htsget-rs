use std::str::FromStr;

use http::uri::Authority;
use serde::{Deserialize, Serialize};

use crate::config::{default_localstorage_addr, default_path, DataServerConfig};
use crate::storage::object::ObjectType;
use crate::tls::KeyPairScheme;
use crate::types::Scheme;

pub(crate) fn default_authority() -> Authority {
  Authority::from_static(default_localstorage_addr())
}

fn default_local_path() -> String {
  default_path().into()
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct LocalStorage {
  scheme: Scheme,
  #[serde(with = "http_serde::authority")]
  authority: Authority,
  local_path: String,
  path_prefix: String,
  object_type: ObjectType,
}

impl LocalStorage {
  /// Create a new local storage.
  pub fn new(
    scheme: Scheme,
    authority: Authority,
    local_path: String,
    path_prefix: String,
    object_type: ObjectType,
  ) -> Self {
    Self {
      scheme,
      authority,
      local_path,
      path_prefix,
      object_type,
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

  /// Get the object type.
  pub fn object_type(&self) -> &ObjectType {
    &self.object_type
  }
}

impl Default for LocalStorage {
  fn default() -> Self {
    Self {
      scheme: Scheme::Http,
      authority: default_authority(),
      local_path: default_local_path(),
      path_prefix: Default::default(),
      object_type: Default::default(),
    }
  }
}

impl From<&DataServerConfig> for Option<LocalStorage> {
  fn from(config: &DataServerConfig) -> Self {
    Some(LocalStorage::new(
      config.tls().get_scheme(),
      Authority::from_str(&config.addr().to_string()).ok()?,
      config.local_path().to_str()?.to_string(),
      config.serve_at().to_string(),
      Default::default(),
    ))
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
        local_path = "path"
        scheme = "HTTPS"
        path_prefix = "path"
        "#,
      |config| {
        println!("{:?}", config.resolvers().first().unwrap().storage());
        assert!(matches!(
            config.resolvers().first().unwrap().storage(),
            Storage::Local { local_storage } if local_storage.local_path() == "path" && local_storage.scheme() == Scheme::Https && local_storage.path_prefix() == "path"
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
    let result: Option<LocalStorage> = (&data_server_config).into();
    let expected = LocalStorage::new(
      Http,
      Authority::from_static("127.0.0.1:8080"),
      "data".to_string(),
      "/data".to_string(),
      Default::default(),
    );

    assert_eq!(result.unwrap(), expected);
  }
}
