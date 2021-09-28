//! Module providing an implementation of the [HtsGet] trait using a [Storage].
//!

use crate::htsget::blocking::search::Search;
use crate::{
  htsget::blocking::bam_search::BamSearch,
  htsget::blocking::bcf_search::BcfSearch,
  htsget::blocking::cram_search::CramSearch,
  htsget::blocking::vcf_search::VcfSearch,
  htsget::blocking::HtsGet,
  htsget::{Format, HtsGetError, Query, Response, Result},
  storage::blocking::Storage,
};

/// Implementation of the [HtsGet] trait using a [Storage].
pub struct HtsGetFromStorage<S> {
  storage: S,
}

impl<S> HtsGet for HtsGetFromStorage<S>
where
  S: Storage,
{
  fn search(&self, query: Query) -> Result<Response> {
    match query.format {
      Some(Format::Bam) | None => BamSearch::new(&self.storage).search(query),
      Some(Format::Cram) => CramSearch::new(&self.storage).search(query),
      Some(Format::Vcf) => VcfSearch::new(&self.storage).search(query),
      Some(Format::Bcf) => BcfSearch::new(&self.storage).search(query),
      Some(Format::Unsupported(format)) => Err(HtsGetError::unsupported_format(format)),
    }
  }

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

impl<S> HtsGetFromStorage<S> {
  pub fn new(storage: S) -> Self {
    Self { storage }
  }

  pub fn storage(&self) -> &S {
    &self.storage
  }
}
