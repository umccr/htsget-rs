//! Module providing a representation of the HtsGet specification.
//!
//! Based on the [HtsGet Specification](https://samtools.github.io/hts-specs/htsget.html).
//!

use crate::htsget::{Format, HtsGetError, Query, Response};

pub mod bam_search;
pub mod bcf_search;
pub mod cram_search;
pub mod from_storage;
pub mod search;
pub mod vcf_search;

type Result<T> = core::result::Result<T, HtsGetError>;

/// Trait representing a search for either `reads` or `variants` in the HtsGet specification.
pub trait HtsGet {
  fn search(&self, query: Query) -> Result<Response>;
  fn get_supported_formats(&self) -> Vec<Format>;
  fn are_field_parameters_effective(&self) -> bool;
  fn are_tag_parameters_effective(&self) -> bool;
}
