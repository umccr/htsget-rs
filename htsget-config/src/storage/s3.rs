use crate::storage::ResolvedId;
use serde::{Deserialize, Serialize};
use std::path::{Component, PathBuf};

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

impl From<ResolvedId> for Option<S3Storage> {
  fn from(resolved_id: ResolvedId) -> Self {
    let path = PathBuf::from(resolved_id.0);
    let path_segment = path.components().find_map(|component| match component {
      Component::Normal(component) => component.to_str(),
      _ => None,
    })?;

    Some(S3Storage::new(path_segment.to_string()))
  }
}

#[cfg(test)]
pub mod tests {
  use super::*;
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

  #[test]
  fn s3_storage_from_data_server_config() {
    let resolved_id = "/bucket/id";
    let result: Option<S3Storage> = ResolvedId(resolved_id.to_string()).into();
    let expected = S3Storage::new("bucket".to_string());

    assert_eq!(result.unwrap(), expected);
  }
}
