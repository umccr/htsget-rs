//! Module providing a representation of the HtsGet specification.
//!
//! Based on the [HtsGet Specification](https://samtools.github.io/hts-specs/htsget.html).
//!

pub use htsget_config::config::Config;
pub use htsget_config::resolver::{IdResolver, ResolveResponse, StorageResolver};
pub use htsget_config::types::{
  Class, Format, Headers, HtsGetError, JsonResponse, Query, Response, Result, Url,
};
pub use htsget_storage::Storage;

pub use htsget_storage::local::FileStorage;

use async_trait::async_trait;
use tokio::task::JoinError;

pub mod bam_search;
pub mod bcf_search;
pub mod cram_search;
pub mod from_storage;
pub mod search;
pub mod vcf_search;

/// Trait representing a search for either `reads` or `variants` in the HtsGet specification.
#[async_trait]
pub trait HtsGet {
  async fn search(self, query: Query) -> Result<Response>;

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
