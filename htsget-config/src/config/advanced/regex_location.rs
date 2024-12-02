//! Set the location using a regex and substitution values.
//!

use regex::Regex;
use serde::{Deserialize, Serialize};

/// A regex storage is a storage that matches ids using Regex.
#[derive(Serialize, Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RegexLocation {
  #[serde(with = "serde_regex")]
  regex: Regex,
  substitution_string: String,
}

impl RegexLocation {
  /// Create a new regex location.
  pub fn new(regex: Regex, substitution_string: String) -> Self {
    Self {
      regex,
      substitution_string,
    }
  }

  /// Get the regex.
  pub fn regex(&self) -> &Regex {
    &self.regex
  }

  /// Get the substitution string.
  pub fn substitution_string(&self) -> &str {
    &self.substitution_string
  }
}

impl Default for RegexLocation {
  fn default() -> Self {
    Self::new(
      ".*".parse().expect("expected valid regex"),
      "$0".to_string(),
    )
  }
}
