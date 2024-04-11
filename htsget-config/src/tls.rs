use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use hyper_rustls::ConfigBuilderExt;
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use rustls_cert_file_reader::{ FileReader, ReadCerts };
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use rustls_native_certs::load_native_certs;
use rustls_pemfile::read_one;
use serde::{Deserialize, Serialize};

use crate::error::Error::{IoError, ParseError};
use crate::error::{Error, Result};
use crate::types::Scheme;
use crate::types::Scheme::{Http, Https};

/// A trait to determine which scheme a key pair option has.
#[async_trait]
pub trait KeyPairScheme {
  /// Get the scheme.
  fn get_scheme(&self) -> Scheme;
}

/// A certificate and key pair used for TLS. Serialization is not implemented because there
/// is no way to convert back to a `PathBuf`.
#[derive(Deserialize, Debug, Clone)]
#[serde(try_from = "CertificateKeyPairPath")]
pub struct TlsServerConfig {
  server_config: ServerConfig,
}

/// A certificate and key pair used for TLS. Serialization is not implemented because there
/// is no way to convert back to a `PathBuf`.
#[derive(Deserialize, Debug, Clone)]
#[serde(try_from = "RootCertStorePair")]
pub struct TlsClientConfig {
  client_config: ClientConfig,
}

impl Default for TlsClientConfig {
  fn default() -> Self {
    Self {
      client_config: ClientConfig::builder().with_native_roots()
                                            .unwrap()
                                            .with_no_client_auth(),
    }
  }
}

impl TlsServerConfig {
  /// Create a new TlsServerConfig.
  pub fn new(server_config: ServerConfig) -> Self {
    Self { server_config }
  }

  /// Get the inner server config.
  pub fn into_inner(self) -> ServerConfig {
    self.server_config
  }
}

impl TlsClientConfig {
  /// Create a new TlsClientConfig.
  pub fn new(client_config: ClientConfig) -> Self {
    Self { client_config }
  }

  /// Get the inner client config.
  pub fn into_inner(self) -> ClientConfig {
    self.client_config
  }
}

/// The location of a certificate and key pair used for TLS.
/// This is the path to the PEM formatted X.509 certificate and private key.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CertificateKeyPairPath {
  cert: PathBuf,
  key: PathBuf,
}

/// The certificate and key pair used for TLS.
#[derive(Debug, PartialEq, Eq)]
pub struct CertificateKeyPair {
  certs: Vec<CertificateDer<'static>>,
  key: PrivateKeyDer<'static>,
}

impl CertificateKeyPair {
  /// Create a new CertificateKeyPair.
  pub fn new(certs: Vec<CertificateDer>, key: PrivateKeyDer) -> Self {
    Self { certs, key }
  }

  /// Get the owned certificate and private key.
  pub fn into_inner(self) -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>) {
    (self.certs, self.key)
  }
}

/// The location of a certificate and key pair used for TLS.
/// This is the path to the PEM formatted X.509 certificate and private key.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RootCertStorePair {
  #[serde(flatten)]
  key_pair: Option<CertificateKeyPairPath>,
  root_store: Option<PathBuf>,
}

impl RootCertStorePair {
  /// Create a new RootCertStorePair.
  pub fn new(key_pair: Option<CertificateKeyPairPath>, root_store: Option<PathBuf>) -> Self {
    Self {
      key_pair,
      root_store,
    }
  }

  /// Get the owned root store pair.
  pub fn into_inner(self) -> (Option<CertificateKeyPairPath>, Option<PathBuf>) {
    (self.key_pair, self.root_store)
  }
}

impl TryFrom<CertificateKeyPairPath> for TlsServerConfig {
  type Error = Error;

  fn try_from(key_pair: CertificateKeyPairPath) -> Result<Self> {
    let server_config = tls_server_config(key_pair.try_into()?)?;

    Ok(Self::new(server_config))
  }
}

impl TryFrom<CertificateKeyPairPath> for CertificateKeyPair {
  type Error = Error;

  fn try_from(key_pair: CertificateKeyPairPath) -> Result<Self> {
    let certs = load_certs(key_pair.cert);
    let key = load_key(key_pair.key);

    Ok(CertificateKeyPair::new(certs, key))
  }
}

impl TryFrom<RootCertStorePair> for TlsClientConfig {
  type Error = Error;

  fn try_from(root_store_pair: RootCertStorePair) -> Result<Self> {
    let (key_pair, root_store) = root_store_pair.into_inner();

    let root_certs = Some(load_root_store_from_path(root_store.expect("Could not load some root certificate")));
    let key_pair = key_pair.map(TryInto::try_into).transpose()?;

    tls_client_config(key_pair, root_certs).map(Self::new)
  }
}

impl CertificateKeyPairPath {
  /// Create a new certificate key pair.
  pub fn new(cert: PathBuf, key: PathBuf) -> Self {
    Self { cert, key }
  }

  /// Get the certs path.
  pub fn certs(&self) -> &Path {
    &self.cert
  }

  /// Get the key path.
  pub fn key(&self) -> &Path {
    &self.key
  }
}

impl KeyPairScheme for Option<&TlsServerConfig> {
  fn get_scheme(&self) -> Scheme {
    match self {
      None => Http,
      Some(_) => Https,
    }
  }
}

/// Load a private key from a file. Supports RSA, PKCS8, and Sec1 encoded keys.
pub async fn load_key<P: AsRef<Path>>(key: P) -> Result<PrivateKeyDer<'static>> {
  let mut key_reader = BufReader::new(
    File::open(key).map_err(|err| IoError(format!("failed to open key file: {}", err)))?,
  );

  loop {
    match read_one(&mut key_reader)
      .map_err(|err| ParseError(format!("failed to parse private key: {}", err)))?
    {
      Some(rustls_pemfile::Item::Pkcs8Key(key)) => return Ok(PrivateKeyDer::Pkcs8(key)),
      None => break,
      _ => {}
    }
  }

  Err(ParseError("no key found in pem file".to_string()))
}

/// Load certificates from a file.
pub async fn load_certs(certs: PathBuf) -> Result<Vec<CertificateDer<'static>>> {
  let certs = FileReader::new(certs, rustls_cert_file_reader::Format::DER).read_certs().await;

  Ok(certs.map_err(|err| ParseError(err.to_string()))?)
}

/// Load certificates from a file and place them in a root CA store.
pub async fn load_root_store_from_path(certs: PathBuf) -> Result<RootCertStore> {
  let certs = load_certs(certs).await?.into_iter();
  let mut root_store = RootCertStore::empty();

  for cert in certs {
    root_store.add(CertificateDer::from(cert));
  }

  Ok(root_store)
}

/// Load certificates and place them in a root CA store.
pub fn load_root_store(certs: Vec<Vec<u8>>) -> Result<RootCertStore> {
  let mut root_store = RootCertStore::empty();

  for cert in certs {
    root_store.add(CertificateDer::from(cert));
  }

  Ok(root_store)
}

/// Load TLS server config.
pub fn tls_server_config(key_pair: CertificateKeyPair) -> Result<ServerConfig> {
  let (certs, key) = key_pair.into_inner();

  let mut config = ServerConfig::builder()
    .with_no_client_auth()
    .with_single_cert(certs, key)
    .map_err(|err| ParseError(err.to_string()))?;

  config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

  Ok(config)
}

/// Load TLS client config. Defaults to the system's native root store if `root_store` is `None`,
/// and to no client authentication if `key_pair` is `None`.
pub fn tls_client_config(
  key_pair: Option<CertificateKeyPair>,
  root_store: Option<RootCertStore>,
) -> Result<ClientConfig> {
  let config = ClientConfig::builder();

  let config = if let Some(root_store) = root_store {
      config.with_root_certificates(root_store)
    } else {
      let certs =
        load_native_certs().map_err(|err| ParseError(format!("loading native certs: {}", err)))?;

        let mut root_store = RootCertStore::empty();

        for cert in certs {
          root_store.add(CertificateDer::from(cert));
        }

      config.with_root_certificates(root_store)
  };

  // let config = if let Some(key_pair) = key_pair {
  //   config
  //     .with_client_auth_cert(key_pair.certs, key_pair.key)
  //     .map_err(|err| ParseError(format!("single cert: {}", err)))?
  // } else {
  //   config.with_no_client_auth()
  // };

  // No need to define ALPN protocols for hyper-rustls connector.

  Ok(config.with_no_client_auth())
}

#[cfg(test)]
pub(crate) mod tests {
  use std::fs::write;
  use std::path::Path;

  use rcgen::generate_simple_self_signed;
  use tempfile::TempDir;

  use super::*;

  #[tokio::test]
  async fn test_load_key() {
    with_test_certificates(|path: &Path, key, _| {
      let key_path = path.join("key.pem");
      let loaded_key = load_key(key_path).await?;

      assert_eq!(loaded_key, key);
    });
  }

  #[tokio::test]
  async fn test_load_cert() {
    with_test_certificates(|path: &Path, _, cert| {
      let cert_path = path.join("cert.pem");
      let certs = load_certs(cert_path).await?;

      assert_eq!(certs.len(), 1);
      assert_eq!(certs.into_iter().next().unwrap(), cert);
    });
  }

  #[tokio::test]
  async fn test_load_root_ca() {
    with_test_certificates(|path: &Path, _, _| {
      let cert_path = path.join("cert.pem");
      let certs = load_root_store_from_path(cert_path).await?;

      assert_eq!(certs.len(), 1);
    });
  }

  #[tokio::test]
  async fn test_tls_server_config() {
    with_test_certificates(|_, key, cert| {
      let server_config = tls_server_config(CertificateKeyPair::new(vec![cert], key)).unwrap();

      assert_eq!(
        server_config.alpn_protocols,
        vec![b"h2".to_vec(), b"http/1.1".to_vec()]
      );
    });
  }

  #[tokio::test]
  async fn test_tls_client_config() {
    with_test_certificates(|path: &Path, key, cert| {
      let certs = load_root_store_from_path(path.join("cert.pem")).await?;
      let client_config =
        tls_client_config(Some(CertificateKeyPair::new(vec![cert], key)), Some(certs));

      assert!(client_config.is_ok());
    });
  }

  pub(crate) fn with_test_certificates<F>(test: F)
  where
    F: FnOnce(&Path, PrivateKeyDer, CertificateDer),
  {
    let tmp_dir = TempDir::new().unwrap();

    let key_path = tmp_dir.path().join("key.pem");
    let cert_path = tmp_dir.path().join("cert.pem");

    let cert = generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();

    let key = cert.key_pair.serialized_der();
    let cert = cert.key_pair.serialized_der();

    write(key_path, &key).unwrap();
    write(cert_path, &cert).unwrap();

    let key = PrivateKeyDer::Pkcs8(key.into());
    let cert = CertificateDer::from(cert);

    test(tmp_dir.path(), key, cert);
  }
}
