//! Configuration options that are advanced in the documentation.
//!

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crate::tls::client::TlsClientConfig;
use reqwest::Client;
use serde::{Deserialize, Serialize};

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
