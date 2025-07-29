//! Configuration options that are advanced in the documentation.
//!

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crate::tls::client::TlsClientConfig;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub mod allow_guard;
pub mod auth;
pub mod cors;
pub mod regex_location;
#[cfg(feature = "url")]
pub mod url;

/// Determines which tracing formatting style to use.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub enum FormattingStyle {
  #[default]
  Full,
  Compact,
  Pretty,
  Json,
}

/// A wrapper around a reqwest client to support creating from config fields.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields, try_from = "TlsClientConfig")]
pub struct HttpClient(Client);

impl HttpClient {
  /// Create a new client.
  pub fn new(client: Client) -> Self {
    Self(client)
  }

  /// Get the inner client value.
  pub fn into_inner(self) -> Client {
    self.0
  }
}

impl TryFrom<TlsClientConfig> for HttpClient {
  type Error = Error;

  fn try_from(config: TlsClientConfig) -> Result<Self> {
    let mut builder = Client::builder();

    let (certs, identity) = config.into_inner();

    if let Some(certs) = certs {
      for cert in certs {
        builder = builder.add_root_certificate(cert);
      }
    }
    if let Some(identity) = identity {
      builder = builder.identity(identity);
    }

    Ok(Self(builder.build().map_err(|err| {
      ParseError(format!("building http client: {err}"))
    })?))
  }
}

/// A wrapper around byte data to support reading files in config.
pub struct Bytes(Vec<u8>);

impl Bytes {
  /// Create a new data wrapper.
  pub fn new(data: Vec<u8>) -> Self {
    Self(data)
  }

  /// Get the bytes.
  pub fn into_inner(self) -> Vec<u8> {
    self.0
  }
}

impl TryFrom<PathBuf> for Bytes {
  type Error = Error;

  fn try_from(path: PathBuf) -> Result<Self> {
    let mut bytes = vec![];
    File::open(path)?.read_to_end(&mut bytes)?;
    Ok(Self(bytes))
  }
}
