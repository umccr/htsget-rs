use serde::{Deserialize, Serialize};

use crate::storage::ResolverMatcher;
use tracing::instrument;

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct S3Storage {
  bucket: String,
  endpoint: Option<String>,
}

impl S3Storage {
  /// Create a new S3 storage.
  pub fn new(bucket: String, endpoint: Option<String>) -> Self {
    Self { bucket, endpoint }
  }

  /// Get the bucket.
  pub fn bucket(&self) -> &str {
    &self.bucket
  }

  /// Get the endpoint
  pub fn endpoint(self) -> Option<String> {
    self.endpoint
  }
}

impl<'a> From<ResolverMatcher<'a>> for Option<S3Storage> {
  #[instrument(level = "trace", ret)]
  fn from(resolver_and_query: ResolverMatcher) -> Self {
    let (regex, regex_match) = resolver_and_query.into_inner();
    let bucket = regex.captures(regex_match)?.get(1)?.as_str();
    let endpoint = None;

    Some(S3Storage::new(bucket.to_string(), endpoint))
  }
}

#[cfg(test)]
mod tests {
  use regex::Regex;

  use crate::config::tests::test_config_from_file;
  use crate::storage::Storage;

  use super::*;

  #[test]
  fn config_storage_s3_file() {
    test_config_from_file(
      r#"
        [[resolvers]]
        regex = "regex"

        [resolvers.storage]
        bucket = "bucket"
        "#,
      |config| {
        println!("{:?}", config.resolvers().first().unwrap().storage());
        assert!(matches!(
            config.resolvers().first().unwrap().storage(),
            Storage::S3 { s3_storage } if s3_storage.bucket() == "bucket"
        ));
      },
    );
  }

  #[test]
  fn s3_storage_from_resolver_and_query() {
    let regex = Regex::new("^(bucket)/(?P<key>.*)$").unwrap();

    let result: Option<S3Storage> = ResolverMatcher(&regex, "bucket/id").into();
    let expected = S3Storage::new("bucket".to_string(), None); // TODO: Fix custom endpoint func

    assert_eq!(result.unwrap(), expected);
  }

  #[test]
  fn s3_storage_from_resolver_and_query_no_captures() {
    let regex = Regex::new("^bucket/id$").unwrap();

    let result: Option<S3Storage> = ResolverMatcher(&regex, "/bucket/id").into();

    assert_eq!(result, None);
  }
}
