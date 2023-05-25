//! Module providing a representation of the HtsGet specification.
//!
//! Based on the [HtsGet Specification](https://samtools.github.io/hts-specs/htsget.html).
//!

use async_trait::async_trait;
use tokio::task::JoinError;

use htsget_config::types::{Format, HtsGetError, Query, Response, Result};

use crate::storage::StorageError;

pub mod bam_search;
pub mod bcf_search;
pub mod cram_search;
pub mod from_storage;
pub mod search;
pub mod vcf_search;

/// Trait representing a search for either `reads` or `variants` in the HtsGet specification.
#[async_trait]
pub trait HtsGet {
  async fn search(&self, query: Query) -> Result<Response>;

  fn get_supported_formats(&self) -> Vec<Format> {
    vec![Format::Bam, Format::Cram, Format::Vcf, Format::Bcf]
  }

  fn are_field_parameters_effective(&self) -> bool {
    false
  }

  fn are_tag_parameters_effective(&self) -> bool {
    false
  }
}

impl From<StorageError> for HtsGetError {
  fn from(err: StorageError) -> Self {
    match err {
      err @ StorageError::InvalidInput(_) => Self::InvalidInput(err.to_string()),
      err @ (StorageError::KeyNotFound(_)
      | StorageError::InvalidKey(_)
      | StorageError::ResponseError(_)) => Self::NotFound(err.to_string()),
      err @ StorageError::IoError(_, _) => Self::IoError(err.to_string()),
      err @ (StorageError::ServerError(_)
      | StorageError::InvalidUri(_)
      | StorageError::InvalidAddress(_)
      | StorageError::InternalError(_)) => Self::InternalError(err.to_string()),
      #[cfg(feature = "s3-storage")]
      err @ StorageError::AwsS3Error(_, _) => Self::IoError(err.to_string()),
      err @ StorageError::UrlParseError(_) => Self::ParseError(err.to_string()),
    }
  }
}

pub(crate) struct ConcurrencyError(JoinError);

impl ConcurrencyError {
  /// Create a new concurrency error.
  pub fn new(error: JoinError) -> Self {
    Self(error)
  }

  /// Get the inner join error.
  pub fn into_inner(self) -> JoinError {
    self.0
  }
}

impl From<ConcurrencyError> for HtsGetError {
  fn from(err: ConcurrencyError) -> Self {
    Self::internal_error(err.into_inner().to_string())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn htsget_error_from_storage_not_found() {
    let result = HtsGetError::from(StorageError::KeyNotFound("error".to_string()));
    assert!(matches!(result, HtsGetError::NotFound(_)));
  }

  #[test]
  fn htsget_error_from_storage_invalid_key() {
    let result = HtsGetError::from(StorageError::InvalidKey("error".to_string()));
    assert!(matches!(result, HtsGetError::NotFound(_)));
  }
}
