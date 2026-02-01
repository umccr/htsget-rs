//! Sources a Crypt4GH key from a header incoming with the initial htsget request.
//!

use crate::config::advanced::CONTEXT_HEADER_PREFIX;
use crate::types::HtsGetError;
use crate::types::HtsGetError::InvalidInput;
use base64::Engine;
use base64::engine::general_purpose;
use crypt4gh::keys::get_public_key;
use http::HeaderMap;
use schemars::JsonSchema;
use serde::Deserialize;
use std::fs;
use tempfile::NamedTempFile;

/// Specify keys from a header
#[derive(JsonSchema, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct C4GHHeader;

impl C4GHHeader {
  /// Returns `Htsget-Context-Public-Key`.
  pub fn format_header_name() -> String {
    format!("{CONTEXT_HEADER_PREFIX}Public-Key")
  }

  /// Get the public key from the header.
  pub fn get_public_key(self, headers: &HeaderMap) -> Result<Vec<u8>, HtsGetError> {
    let header_name = Self::format_header_name();
    let public_key = headers.get(&header_name).ok_or(InvalidInput(header_name))?;
    let public_key = general_purpose::STANDARD
      .decode(public_key.as_ref())
      .map_err(|err| InvalidInput(err.to_string()))?;

    let tmp = NamedTempFile::new()?;
    fs::write(tmp.path(), public_key)?;

    get_public_key(tmp.path().to_path_buf()).map_err(|err| InvalidInput(err.to_string()))
  }

  /// Encode a public key using base64.
  pub fn encode_public_key(public_key: String) -> String {
    general_purpose::STANDARD.encode(public_key)
  }
}
