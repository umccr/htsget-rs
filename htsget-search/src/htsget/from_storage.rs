//! Module providing an implementation of the [HtsGet] trait using a [Storage].
//!

use std::sync::Arc;

use async_trait::async_trait;

use crate::htsget::search::Search;
use crate::{
  htsget::bam_search::BamSearch,
  htsget::bcf_search::BcfSearch,
  htsget::cram_search::CramSearch,
  htsget::vcf_search::VcfSearch,
  htsget::{Format, HtsGet, HtsGetError, Query, Response, Result},
  storage::AsyncStorage,
};

/// Implementation of the [HtsGet] trait using a [Storage].
pub struct HtsGetFromStorage<S> {
  storage_ref: Arc<S>,
}

#[async_trait]
impl<S> HtsGet for HtsGetFromStorage<S>
where
  S: AsyncStorage + Sync + Send + 'static,
{
  async fn search(&self, query: Query) -> Result<Response> {
    match query.format {
      Some(Format::Bam) | None => BamSearch::new(self.storage()).search(query).await,
      Some(Format::Cram) => CramSearch::new(self.storage()).search(query).await,
      Some(Format::Vcf) => VcfSearch::new(self.storage()).search(query).await,
      Some(Format::Bcf) => BcfSearch::new(self.storage()).search(query).await,
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
    Self {
      storage_ref: Arc::new(storage),
    }
  }

  pub fn storage(&self) -> Arc<S> {
    Arc::clone(&self.storage_ref)
  }
}
