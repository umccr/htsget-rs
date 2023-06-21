use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use rustls::{Certificate, PrivateKey, RootCertStore};
use rustls_pemfile::read_one;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::error::Error::{IoError, ParseError};
use crate::error::Result;
use crate::types::Scheme;
use crate::types::Scheme::{Http, Https};

/// A trait to determine which scheme a key pair option has.
pub trait KeyPairScheme {
  /// Get the scheme.
  fn get_scheme(&self) -> Scheme;
}

/// A certificate and key pair used for TLS.
/// This is the path to the PEM formatted X.509 certificate and private key.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CertificateKeyPair {
  cert: PathBuf,
  key: PathBuf,
}

impl CertificateKeyPair {
  /// Create a new certificate key pair.
  pub fn new(cert: PathBuf, key: PathBuf) -> Self {
    Self { cert, key }
  }

  /// Get the cert.
  pub fn cert(&self) -> &Path {
    &self.cert
  }

  /// Get the key.
  pub fn key(&self) -> &Path {
    &self.key
  }
}

impl KeyPairScheme for Option<&CertificateKeyPair> {
  fn get_scheme(&self) -> Scheme {
    match self {
      None => Http,
      Some(_) => Https,
    }
  }
}

pub fn load_key<P: AsRef<Path>>(key: P) -> Result<PrivateKey> {
  let mut key_reader = BufReader::new(
    File::open(key).map_err(|err| IoError(format!("failed to open key file: {}", err)))?,
  );

  loop {
    match read_one(&mut key_reader)
      .map_err(|err| ParseError(format!("failed to parse private key: {}", err)))?
    {
      Some(rustls_pemfile::Item::RSAKey(key)) => return Ok(PrivateKey(key)),
      Some(rustls_pemfile::Item::PKCS8Key(key)) => return Ok(PrivateKey(key)),
      Some(rustls_pemfile::Item::ECKey(key)) => return Ok(PrivateKey(key)),
      None => break,
      _ => {}
    }
  }

  Err(ParseError("no key found in pem file".to_string()))
}

fn load_certs<P: AsRef<Path>>(certs: P) -> Result<Vec<Certificate>> {
  let mut cert_reader = BufReader::new(
    File::open(certs).map_err(|err| IoError(format!("failed to open cert file: {}", err)))?,
  );

  let certs: Vec<Certificate> = rustls_pemfile::certs(&mut cert_reader)
    .map_err(|err| ParseError(format!("failed to parse certificates: {}", err)))?
    .into_iter()
    .map(Certificate)
    .collect();

  if certs.is_empty() {
    return Err(ParseError("no certificates found in pem file".to_string()));
  }

  Ok(certs)
}

pub fn load_root_ca<P: AsRef<Path>>(certs: P) -> Result<RootCertStore> {
  let certs: Vec<Vec<u8>> = load_certs(certs)?.into_iter().map(|cert| cert.0).collect();

  let mut roots = RootCertStore::empty();
  let (_, ignored) = roots.add_parsable_certificates(&certs);

  if ignored != 0 {
    warn!("{} certificates ignored when loading root CA", ignored);
  }

  if roots.is_empty() {
    return Err(ParseError(
      "no certificates found in root CA file".to_string(),
    ));
  }

  Ok(roots)
}

#[cfg(test)]
mod tests {
  use std::fs::write;
  use std::io::Cursor;
  use std::path::Path;

  use rcgen::generate_simple_self_signed;
  use rustls_pemfile::pkcs8_private_keys;
  use tempfile::TempDir;

  use super::*;

  #[test]
  fn test_load_key() {
    with_test_certificates(|path| {
      let key_path = path.join("key.pem");
      let key = load_key(key_path).unwrap();

      let mut key_reader = Cursor::new(key.0);

      let result = pkcs8_private_keys(&mut key_reader);
      assert!(result.is_ok());
    });
  }

  #[test]
  fn test_load_cert() {
    with_test_certificates(|path| {
      let cert_path = path.join("cert.pem");
      let certs = load_certs(cert_path).unwrap();

      assert_eq!(certs.len(), 1);
    });
  }

  #[test]
  fn test_load_root_ca() {
    with_test_certificates(|path| {
      let cert_path = path.join("cert.pem");
      let certs = load_root_ca(cert_path).unwrap();

      assert_eq!(certs.len(), 1);
    });
  }

  fn with_test_certificates<F>(test: F)
  where
    F: FnOnce(&Path),
  {
    let tmp_dir = TempDir::new().unwrap();

    let key_path = tmp_dir.path().join("key.pem");
    let cert_path = tmp_dir.path().join("cert.pem");

    let cert = generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    write(key_path, cert.serialize_private_key_pem()).unwrap();
    write(cert_path, cert.serialize_pem().unwrap()).unwrap();

    test(tmp_dir.path());
  }
}
