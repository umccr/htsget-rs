use serde::{Deserialize, Serialize};

use crate::storage::ResolverAndQuery;

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct S3Storage {
  bucket: String,
}

impl S3Storage {
  /// Create a new S3 storage.
  pub fn new(bucket: String) -> Self {
    Self { bucket }
  }

  /// Get the bucket.
  pub fn bucket(&self) -> &str {
    &self.bucket
  }
}

impl<'a> From<ResolverAndQuery<'a>> for Option<S3Storage> {
  fn from(resolver_and_query: ResolverAndQuery) -> Self {
    let (regex, query) = resolver_and_query.into_inner();
    let bucket = regex.captures(query.id())?.get(1)?.as_str();

    Some(S3Storage::new(bucket.to_string()))
  }
}

#[cfg(test)]
mod tests {
  use regex::Regex;

  use crate::config::tests::test_config_from_file;
  use crate::storage::Storage;
  use crate::types::Format::Bam;
  use crate::types::Query;

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
    let query = Query::new("bucket/id", Bam);

    let result: Option<S3Storage> = ResolverAndQuery(&regex, &query).into();
    let expected = S3Storage::new("bucket".to_string());

    assert_eq!(result.unwrap(), expected);
  }

  #[test]
  fn s3_storage_from_resolver_and_query_no_captures() {
    let regex = Regex::new("^bucket/id$").unwrap();
    let query = Query::new("/bucket/id", Bam);

    let result: Option<S3Storage> = ResolverAndQuery(&regex, &query).into();

    assert_eq!(result, None);
  }
}
