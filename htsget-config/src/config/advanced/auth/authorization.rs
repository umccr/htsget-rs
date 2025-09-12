//! Config for handling the authorization flow.
//!

use crate::config::advanced::auth::AuthorizationRestrictions;
use http::Uri;
use http_serde::uri;
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
    let uri = uri::deserialize(deserializer)?;

    if uri
      .scheme_str()
      .is_none_or(|scheme| scheme.to_lowercase() == "file")
    {
      let mut auth_rules = File::open(uri.to_string()).map_err(Error::custom)?;
      let mut buf = vec![];
      auth_rules.read_to_end(&mut buf).map_err(Error::custom)?;
      Ok(UrlOrStatic::Static(
        serde_json::from_slice(buf.as_slice()).map_err(Error::custom)?,
      ))
    } else {
      Ok(UrlOrStatic::Url(uri))
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
