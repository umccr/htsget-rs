//! Set the location using a regex and substitution values.
//!

use crate::config::advanced::allow_guard::AllowGuard;
use crate::config::location::LocationEither;
use crate::storage::Backend;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// A regex storage is a storage that matches ids using Regex.
#[derive(Serialize, Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RegexLocation {
  #[serde(with = "serde_regex")]
  regex: Regex,
  substitution_string: String,
  backend: Backend,
  guard: Option<AllowGuard>,
}

impl RegexLocation {
  /// Create a new regex location.
  pub fn new(
    regex: Regex,
    substitution_string: String,
    backend: Backend,
    guard: Option<AllowGuard>,
  ) -> Self {
    Self {
      regex,
      substitution_string,
      backend,
      guard,
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

  /// Get the storage backend.
  pub fn backend(&self) -> &Backend {
    &self.backend
  }

  /// Get the allow guard.
  pub fn guard(&self) -> Option<&AllowGuard> {
    self.guard.as_ref()
  }
}

impl Default for RegexLocation {
  fn default() -> Self {
    Self::new(
      ".*".parse().expect("expected valid regex"),
      "$0".to_string(),
      Default::default(),
      Default::default(),
    )
  }
}

impl From<RegexLocation> for LocationEither {
  fn from(location: RegexLocation) -> Self {
    Self::Regex(location)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::tests::test_serialize_and_deserialize;

  #[test]
  fn regex_location_file() {
    test_serialize_and_deserialize(
      r#"
      regex = "123-.*"
      substitution_string = "123"
      "#,
      ("123-.*".to_string(), "123".to_string()),
      |result: RegexLocation| {
        result.backend().as_file().unwrap();
        (
          result.regex().as_str().to_string(),
          result.substitution_string().to_string(),
        )
      },
    );
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn regex_location_s3() {
    test_serialize_and_deserialize(
      r#"
      regex = "123-.*"
      substitution_string = "123"
      location.backend = "S3"
      "#,
      ("123-.*".to_string(), "123".to_string()),
      |result: RegexLocation| {
        result.backend().as_s3().unwrap();
        (
          result.regex().as_str().to_string(),
          result.substitution_string().to_string(),
        )
      },
    );
  }

  #[cfg(feature = "url-storage")]
  #[test]
  fn regex_location_url() {
    test_serialize_and_deserialize(
      r#"
      regex = "123-.*"
      substitution_string = "123"

      [location]
      backend = "Url"
      url = "https://example.com"
      "#,
      (
        "123-.*".to_string(),
        "123".to_string(),
        "https://example.com/".to_string(),
      ),
      |result: RegexLocation| {
        let url = result.backend().as_url().unwrap();

        (
          result.regex().as_str().to_string(),
          result.substitution_string().to_string(),
          url.url().to_string(),
        )
      },
    );
  }
}
