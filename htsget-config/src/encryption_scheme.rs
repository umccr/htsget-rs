//! Types related to the experimental encryption scheme.
//!

use serde::{Deserialize, Serialize};

/// The file encryption scheme
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncryptionScheme {
  C4GH,
}
