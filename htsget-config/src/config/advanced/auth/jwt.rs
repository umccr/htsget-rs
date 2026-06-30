//! JWT key config.
//!

use crate::config::advanced::Bytes;
use crate::config::advanced::callout::Callout;
use crate::error::Result;
use serde::Deserialize;
use std::path::PathBuf;

/// The source for the JWT signing key, either a JWKS endpoint or a
/// static public key.
#[derive(Debug, Clone)]
pub enum JwtKey {
  Jwks(Box<Callout>),
  PublicKey(Vec<u8>),
}

impl JwtKey {
  /// Get the JWKS callout if the type is `Jwks`.
  pub fn jwks(&self) -> Option<&Callout> {
    match self {
      Self::Jwks(callout) => Some(callout),
      Self::PublicKey(_) => None,
    }
  }

  /// Get a mutable reference to the JWKS callout if the type is `Jwks`.
  pub fn jwks_mut(&mut self) -> Option<&mut Callout> {
    match self {
      Self::Jwks(callout) => Some(callout),
      Self::PublicKey(_) => None,
    }
  }

  /// Get the public key bytes if the type is `PublicKey`.
  pub fn public_key(&self) -> Option<&[u8]> {
    match self {
      Self::PublicKey(key) => Some(key),
      Self::Jwks(_) => None,
    }
  }
}

/// Builder for `JwtKey`.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum JwtKeyBuilder {
  Jwks(Box<Callout>),
  PublicKey { path: PathBuf },
}

impl JwtKeyBuilder {
  /// Build the `JwtKey`.
  pub fn build(self) -> Result<JwtKey> {
    match self {
      Self::Jwks(callout) => Ok(JwtKey::Jwks(callout)),
      Self::PublicKey { path } => Ok(JwtKey::PublicKey(Bytes::try_from(path)?.into_inner())),
    }
  }
}
