//! Config for handling the authorization flow.
//!

use crate::config::advanced::auth::AuthorizationRestrictions;
use http::Uri;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use std::fs::File;
use std::io::Read;

/// The authorization restrictions to fetch from either a URL or a hard-coded
/// static config.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum UrlOrStatic {
  Url(Uri),
  Static(AuthorizationRestrictions),
}

impl<'de> Deserialize<'de> for UrlOrStatic {
  fn deserialize<D>(deserializer: D) -> Result<UrlOrStatic, D::Error>
  where
    D: Deserializer<'de>,
  {
    let uri = String::deserialize(deserializer)?;

    if uri.to_lowercase().starts_with("http://") || uri.to_lowercase().starts_with("https://") {
      Ok(UrlOrStatic::Url(uri.parse().map_err(Error::custom)?))
    } else {
      let mut auth_rules =
        File::open(uri.strip_prefix("file://").unwrap_or(&uri)).map_err(Error::custom)?;
      let mut buf = vec![];
      auth_rules.read_to_end(&mut buf).map_err(Error::custom)?;
      Ok(UrlOrStatic::Static(
        serde_json::from_slice(buf.as_slice()).map_err(Error::custom)?,
      ))
    }
  }
}

/// The extensions to pass through to the authorization server from http request extensions.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ForwardExtensions {
  json_path: String,
  name: String,
}

impl ForwardExtensions {
  /// Create a new forward extensions config.
  pub fn new(json_path: String, name: String) -> Self {
    Self { json_path, name }
  }

  /// Get the JSON path to fetch for the extension.
  pub fn json_path(&self) -> &str {
    &self.json_path
  }

  /// Get the name of the header.
  pub fn name(&self) -> &str {
    &self.name
  }
}
