//! Types related to the experimental encryption scheme.
//!

use crate::error::Error;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// The file encryption scheme
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncryptionScheme {
  C4GH,
}
