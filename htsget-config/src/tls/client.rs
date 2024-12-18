//! TLS configuration related to HTTP clients.
//!

use crate::error::Error::IoError;
use crate::error::{Error, Result};
use crate::tls::RootCertStorePair;
use crate::tls::{load_certs, read_bytes};
use reqwest::{Certificate, Identity};
use serde::Deserialize;

/// A certificate and key pair used for TLS. Serialization is not implemented because there
/// is no way to convert back to a `PathBuf`.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(try_from = "RootCertStorePair", deny_unknown_fields)]
pub struct TlsClientConfig {
  cert: Option<Vec<Certificate>>,
  identity: Option<Identity>,
}

impl TlsClientConfig {
  /// Create a new TlsClientConfig.
  pub fn new(cert: Option<Vec<Certificate>>, identity: Option<Identity>) -> Self {
    Self { cert, identity }
  }

  /// Get the inner client config.
  pub fn into_inner(self) -> (Option<Vec<Certificate>>, Option<Identity>) {
    (self.cert, self.identity)
  }
}

impl TryFrom<RootCertStorePair> for TlsClientConfig {
  type Error = Error;

  fn try_from(root_store_pair: RootCertStorePair) -> Result<Self> {
    let (key_pair, root_store) = root_store_pair.into_inner();

    let cert = root_store
      .clone()
      .map(|cert_path| {
        let certs = load_certs(cert_path)?;

        certs
          .into_iter()
          .map(|cert| {
            Certificate::from_der(&cert)
              .map_err(|err| IoError(format!("failed to read certificate from pem: {}", err)))
          })
          .collect::<Result<Vec<_>>>()
      })
      .transpose()?;

    let identity = key_pair
      .clone()
      .map(|pair| {
        let key = read_bytes(pair.key)?;
        let certs = read_bytes(pair.cert)?;

        Identity::from_pem(&[certs, key].concat())
          .map_err(|err| IoError(format!("failed to pkcs8 pem identity: {}", err)))
      })
      .transpose()?;

    Ok(Self::new(cert, identity))
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use crate::tls::tests::with_test_certificates;
  use crate::tls::{CertificateKeyPairPath, RootCertStorePair};
  use std::path::Path;

  use super::*;

  #[tokio::test]
  async fn test_tls_client_config() {
    with_test_certificates(|path, _, _| {
      let client_config = client_config_from_path(path);
      let (certs, identity) = client_config.into_inner();

      assert_eq!(certs.unwrap().len(), 1);
      assert!(identity.is_some());
    });
  }

  pub(crate) fn client_config_from_path(path: &Path) -> TlsClientConfig {
    TlsClientConfig::try_from(RootCertStorePair::new(
      Some(CertificateKeyPairPath::new(
        path.join("cert.pem"),
        path.join("key.pem"),
      )),
      Some(path.join("cert.pem")),
    ))
    .unwrap()
  }
}
