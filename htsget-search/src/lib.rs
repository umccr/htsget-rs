//! Module providing a representation of the HtsGet specification.
//!
//! Based on the [HtsGet Specification](https://samtools.github.io/hts-specs/htsget.html).
//!

pub use htsget_config::config::{Config, DataServerConfig, ServiceInfo, TicketServerConfig};
pub use htsget_config::resolver::{
  IdResolver, QueryAllowed, ResolveResponse, Resolver, StorageResolver,
};
pub use htsget_config::storage::Storage as ConfigStorage;
pub use htsget_config::types::{
  Class, Format, Headers, HtsGetError, JsonResponse, Query, Response, Result, Url,
};
pub use htsget_storage::Storage;

pub use htsget_storage::local::LocalStorage;

use std::fmt::Display;
use std::str::FromStr;

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

/// A struct to represent a parsed header
pub struct ParsedHeader<T>(T);

impl<T> ParsedHeader<T> {
  /// Get the inner header value.
  pub fn into_inner(self) -> T {
    self.0
  }
}

impl<T> FromStr for ParsedHeader<T>
where
  T: FromStr,
  <T as FromStr>::Err: Display,
{
  type Err = HtsGetError;

  fn from_str(header: &str) -> Result<Self> {
    Ok(ParsedHeader(header.parse::<T>().map_err(|err| {
      HtsGetError::parse_error(format!("parsing header: {}", err))
    })?))
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
