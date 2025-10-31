//! Sources a Crypt4GH key from a header incoming with the initial htsget request.
//!

use crate::config::advanced::CONTEXT_HEADER_PREFIX;
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
  /// Get the public key from the header.
  pub fn get_public_key(self, headers: &HeaderMap) -> Option<Vec<u8>> {
    let public_key = headers.get(format!("{CONTEXT_HEADER_PREFIX}Public-Key"))?;

    let tmp = NamedTempFile::new().ok()?;
    fs::write(tmp.path(), public_key).ok();

    get_public_key(tmp.path().to_path_buf()).ok()
  }
}
