//! Set the location using a regex and substitution values.
//!

use crate::config::advanced::allow_guard::AllowGuard;
use crate::config::location::Location;
use crate::storage::Backend;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Specify that the location is a regex location that can arbitrarily map IDs using regex strings.
#[derive(JsonSchema, Serialize, Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RegexLocation {
  /// The regex to match the id against.
  #[schemars(with = "Option::<String>", default = "default_schema_none")]
  #[serde(with = "serde_regex")]
  regex: Regex,
  /// A substitution string to find the data when using a location.
  #[schemars(with = "Option::<String>", default = "default_schema_none")]
  substitution_string: String,
  /// The backend of the location if configured.
  #[schemars(with = "Option::<Backend>", default = "default_schema_none")]
  backend: Backend,
  #[schemars(skip)]
  guard: Option<AllowGuard>,
}

fn default_schema_none() -> Option<String> {
  None
}

impl Eq for RegexLocation {}
impl PartialEq for RegexLocation {
  fn eq(&self, other: &Self) -> bool {
    self.substitution_string == other.substitution_string
      && self.backend == other.backend
      && self.guard == other.guard
      && self.regex.to_string() == other.regex.to_string()
  }
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

  /// Get the storage backend as a mutable reference.
  pub fn backend_mut(&mut self) -> &mut Backend {
    &mut self.backend
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

impl From<RegexLocation> for Location {
  fn from(location: RegexLocation) -> Self {
    Self::Regex(Box::new(location))
  }
}

#[cfg(test)]
mod tests {
  use crate::config::Config;
  use crate::config::tests::test_serialize_and_deserialize;

  #[test]
  fn regex_location_file() {
    test_serialize_and_deserialize(
      r#"
      [[locations]]
      regex = "123-.*"
      substitution_string = "123"
      "#,
      ("123-.*".to_string(), "123".to_string()),
      |result: Config| {
        let location = result.locations.into_inner();
        let location = location[0].as_regex().unwrap();
        location.backend().as_file().unwrap();
        (
          location.regex().as_str().to_string(),
          location.substitution_string().to_string(),
        )
      },
    );
  }

  #[cfg(feature = "aws")]
  #[test]
  fn regex_location_s3() {
    test_serialize_and_deserialize(
      r#"
      [[locations]]
      regex = "123-.*"
      substitution_string = "123"
      backend.kind = "S3"
      backend.bucket = "bucket"
      "#,
      (
        "123-.*".to_string(),
        "123".to_string(),
        "bucket".to_string(),
      ),
      |result: Config| {
        let location = result.locations.into_inner();
        let location = location[0].as_regex().unwrap();
        let backend = location.backend().as_s3().unwrap();
        (
          location.regex().as_str().to_string(),
          location.substitution_string().to_string(),
          backend.bucket().to_string(),
        )
      },
    );
  }

  #[cfg(feature = "url")]
  #[test]
  fn regex_location_url() {
    test_serialize_and_deserialize(
      r#"
      [[locations]]
      regex = "123-.*"
      substitution_string = "123"

      backend.kind = "Url"
      backend.url = "https://example.com"
      "#,
      (
        "123-.*".to_string(),
        "123".to_string(),
        "https://example.com/".to_string(),
      ),
      |result: Config| {
        let location = result.locations.into_inner();
        let location = location[0].as_regex().unwrap();
        let url = location.backend().as_url().unwrap();

        (
          location.regex().as_str().to_string(),
          location.substitution_string().to_string(),
          url.url().to_string(),
        )
      },
    );
  }

  #[cfg(all(feature = "url", feature = "aws"))]
  #[test]
  fn regex_location_multiple() {
    test_serialize_and_deserialize(
      r#"
      [[locations]]
      regex = "prefix/(?P<key>.*)$"
      substitution_string = "$key"
      backend.kind = "S3"
      backend.bucket = "bucket"
      backend.path_style = true
      
      [[locations]]
      regex = ".*"
      substitution_string = "$0"
      backend.kind = "Url"
      backend.url = "http://localhost:8080"
      backend.forward_headers = false
    "#,
      (
        "prefix/(?P<key>.*)$".to_string(),
        "$key".to_string(),
        "bucket".to_string(),
        ".*".to_string(),
        "$0".to_string(),
        "http://localhost:8080/".to_string(),
      ),
      |result: Config| {
        let location = result.locations.into_inner();
        let location_one = location[0].as_regex().unwrap();
        let s3 = location_one.backend().as_s3().unwrap();
        let location_two = location[1].as_regex().unwrap();
        let url = location_two.backend().as_url().unwrap();

        (
          location_one.regex().as_str().to_string(),
          location_one.substitution_string().to_string(),
          s3.bucket().to_string(),
          location_two.regex().as_str().to_string(),
          location_two.substitution_string().to_string(),
          url.url().to_string(),
        )
      },
    );
  }
}
