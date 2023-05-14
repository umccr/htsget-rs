use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
  use crate::config::tests::test_config_from_file;
  use crate::storage::Storage;

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
}
