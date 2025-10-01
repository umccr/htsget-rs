//! TLS configuration related to HTTP clients.
//!

use crate::config::advanced::Bytes;
use crate::error::Error::IoError;
use crate::error::{Error, Result};
use crate::http::RootCertStorePair;
use crate::http::load_certs;
use reqwest::{Certificate, Identity};
use serde::Deserialize;

/// A certificate and key pair used for TLS. Serialization is not implemented because there
/// is no way to convert back to a `PathBuf`.
#[derive(Deserialize, Debug, Clone)]
#[serde(try_from = "RootCertStorePair", deny_unknown_fields)]
pub struct HttpClientConfig {
  cert: Option<Vec<Certificate>>,
  identity: Option<Identity>,
  use_cache: bool,
  user_agent: Option<String>,
}

impl Default for HttpClientConfig {
  fn default() -> Self {
    Self {
      cert: None,
      identity: None,
      use_cache: true,
      user_agent: None,
    }
  }
}

impl HttpClientConfig {
  /// Create a new TlsClientConfig.
  pub fn new(cert: Option<Vec<Certificate>>, identity: Option<Identity>, use_cache: bool) -> Self {
    Self {
      cert,
      identity,
      use_cache,
      ..Default::default()
    }
  }

  /// Get the inner client config.
  pub fn into_inner(
    self,
  ) -> (
    Option<Vec<Certificate>>,
    Option<Identity>,
    bool,
    Option<String>,
  ) {
    (self.cert, self.identity, self.use_cache, self.user_agent)
  }

  /// Set the user agent string.
  pub fn with_user_agent(mut self, user_agent: String) -> Self {
    self.user_agent = Some(user_agent);
    self
  }
}

impl TryFrom<RootCertStorePair> for HttpClientConfig {
  type Error = Error;

  fn try_from(root_store_pair: RootCertStorePair) -> Result<Self> {
    let (key_pair, root_store, use_cache) = root_store_pair.into_inner();

    let cert = root_store
      .clone()
      .map(|cert_path| {
        let certs = load_certs(cert_path)?;

        certs
          .into_iter()
          .map(|cert| {
            Certificate::from_der(&cert)
              .map_err(|err| IoError(format!("failed to read certificate from pem: {err}")))
          })
          .collect::<Result<Vec<_>>>()
      })
      .transpose()?;

    let identity = key_pair
      .clone()
      .map(|pair| {
        let key = Bytes::try_from(pair.key)?.into_inner();
        let certs = Bytes::try_from(pair.cert)?.into_inner();

        Identity::from_pem(&[certs, key].concat())
          .map_err(|err| IoError(format!("failed to load pkcs8 pem identity: {err}")))
      })
      .transpose()?;

    Ok(Self::new(cert, identity, use_cache))
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use crate::http::tests::with_test_certificates;
  use crate::http::{CertificateKeyPairPath, RootCertStorePair};
  use std::path::Path;

  use super::*;

  #[tokio::test]
  async fn test_tls_client_config() {
    with_test_certificates(|path, _, _| {
      let client_config = client_config_from_path(path);
      let (certs, identity, _, _) = client_config.into_inner();

      assert_eq!(certs.unwrap().len(), 1);
      assert!(identity.is_some());
    });
  }

  pub(crate) fn client_config_from_path(path: &Path) -> HttpClientConfig {
    HttpClientConfig::try_from(RootCertStorePair::new(
      Some(CertificateKeyPairPath::new(
        path.join("cert.pem"),
        path.join("key.pem"),
      )),
      Some(path.join("cert.pem")),
      true,
    ))
    .unwrap()
  }
}
