//! Configuration of local file based storage.
//!

use crate::config::data_server::DataServerConfig;
use crate::error::Error;
use crate::error::Error::ParseError;
use crate::error::Result;
use crate::http::KeyPairScheme;
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use crate::types::Scheme;
use http::uri::Authority;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Configure the server to fetch data and return tickets from a local filesystem.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct File {
  /// The ticket response scheme to the data server.
  scheme: Scheme,
  /// The authority of the data server.
  #[schemars(with = "String")]
  #[serde(with = "http_serde::authority")]
  authority: Authority,
  /// The local path to serve files from.
  local_path: String,
  /// The headers to add to ticket responses.
  #[serde(skip)]
  ticket_headers: Vec<String>,
  /// Configure the server to fetch data and return tickets from S3.
  #[cfg(feature = "experimental")]
  #[serde(skip_serializing)]
  keys: Option<C4GHKeys>,
  /// The origin of the tickets, which can be different to the data server address.
  ticket_origin: Option<String>,
  #[serde(skip)]
  pub(crate) is_defaulted: bool,
}

impl Eq for File {}

impl PartialEq for File {
  fn eq(&self, other: &Self) -> bool {
    self.scheme == other.scheme
      && self.authority == other.authority
      && self.local_path == other.local_path
      && self.ticket_headers == other.ticket_headers
      && self.ticket_origin == other.ticket_origin
  }
}

impl File {
  /// Create a new local storage.
  pub fn new(scheme: Scheme, authority: Authority, local_path: String) -> Self {
    Self {
      scheme,
      authority,
      local_path,
      ticket_headers: Vec::new(),
      #[cfg(feature = "experimental")]
      keys: None,
      ticket_origin: None,
      is_defaulted: false,
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

  #[cfg(feature = "experimental")]
  /// Set the C4GH keys.
  pub fn set_keys(&mut self, keys: Option<C4GHKeys>) {
    self.keys = keys;
  }

  #[cfg(feature = "experimental")]
  /// Get the C4GH keys.
  pub fn keys(&self) -> Option<&C4GHKeys> {
    self.keys.as_ref()
  }

  #[cfg(feature = "experimental")]
  /// Get the C4GH keys as an owned value.
  pub fn into_keys(self) -> Option<C4GHKeys> {
    self.keys
  }

  /// Get the ticket origin.
  pub fn ticket_origin(&self) -> Option<&str> {
    self.ticket_origin.as_deref()
  }

  /// Set the local path.
  pub fn set_local_path(mut self, local_path: String) -> Self {
    self.local_path = local_path;
    self
  }

  /// Set the scheme.
  pub fn set_scheme(&mut self, scheme: Scheme) {
    self.scheme = scheme;
  }

  /// Set the authority.
  pub fn set_authority(&mut self, authority: Authority) {
    self.authority = authority;
  }

  /// Set the authority.
  pub fn set_ticket_origin(&mut self, ticket_origin: Option<String>) {
    self.ticket_origin = ticket_origin;
  }

  /// Add a header to add to the ticket.
  pub fn add_ticket_header(&mut self, header: String) {
    self.ticket_headers.push(header);
  }

  /// Get the ticket headers.
  pub fn ticket_headers(&self) -> &[String] {
    &self.ticket_headers
  }
}

impl Default for File {
  fn default() -> Self {
    let mut file = Self::new(Scheme::Http, default_authority(), default_path().into());
    file.is_defaulted = true;
    file
  }
}

impl TryFrom<&DataServerConfig> for File {
  type Error = Error;

  fn try_from(config: &DataServerConfig) -> Result<Self> {
    Ok(Self::new(
      config.tls().get_scheme(),
      Authority::from_str(&config.addr().to_string()).map_err(|err| ParseError(err.to_string()))?,
      config
        .local_path()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| default_path().to_string()),
    ))
  }
}

pub(crate) fn default_authority() -> Authority {
  Authority::from_static(default_localstorage_addr())
}

pub(crate) fn default_localstorage_addr() -> &'static str {
  "127.0.0.1:8081"
}

/// The default data server path.
pub fn default_path() -> &'static str {
  "./"
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::tests::test_serialize_and_deserialize;

  #[test]
  fn file_backend() {
    test_serialize_and_deserialize(
      r#"
      scheme = "Https"
      authority = "127.0.0.1:8083"
      local_path = "path"
      "#,
      (
        "127.0.0.1:8083".to_string(),
        Scheme::Https,
        "path".to_string(),
      ),
      |result: File| {
        (
          result.authority.to_string(),
          result.scheme,
          result.local_path,
        )
      },
    );
  }
}
