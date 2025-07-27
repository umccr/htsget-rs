//! Configuration options that are advanced in the documentation.
//!

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
