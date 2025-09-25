//! JWT authentication config.
//!

use crate::config::advanced::Bytes;
use crate::error::Error;
use crate::error::Error::ParseError;
use crate::error::Result;
use http::Uri;
use serde::Deserialize;
use std::path::PathBuf;

/// The method for authorization, either using a JWKS url or a public key.
#[derive(Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(
  deny_unknown_fields,
  try_from = "AuthModeSerde",
  into = "AuthModeSerde"
)]
pub enum AuthMode {
  Jwks(Uri),
  PublicKey(Vec<u8>),
}

/// Used to deserialize into the `AuthMode` struct.
#[derive(Deserialize, Debug, Clone, Eq, PartialEq, Default)]
#[serde(deny_unknown_fields, default)]
struct AuthModeSerde {
  #[serde(with = "http_serde::option::uri")]
  jwks_url: Option<Uri>,
  public_key: Option<PathBuf>,
}

impl TryFrom<AuthModeSerde> for AuthMode {
  type Error = Error;

  fn try_from(mode: AuthModeSerde) -> Result<Self> {
    match (mode.jwks_url, mode.public_key) {
      (None, None) => Err(ParseError(
        "Either 'jwks_url' or 'public_key' must be set".to_string(),
      )),
      (Some(_), Some(_)) => Err(ParseError(
        "Cannot set both 'jwks_url' and 'public_key'".to_string(),
      )),
      (Some(jwks_url), None) => Ok(AuthMode::Jwks(jwks_url)),
      (None, Some(public_key)) => Ok(AuthMode::PublicKey(
        Bytes::try_from(public_key)?.into_inner(),
      )),
    }
  }
}
