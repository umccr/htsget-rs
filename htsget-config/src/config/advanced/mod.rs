//! Advanced configuration.
//!

use serde::{Deserialize, Serialize};

pub mod allow_guard;
pub mod cors;
pub mod regex_location;
#[cfg(feature = "url-storage")]
pub mod url;

/// Determines which tracing formatting style to use.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Default)]
pub enum FormattingStyle {
  #[default]
  Full,
  Compact,
  Pretty,
  Json,
}
