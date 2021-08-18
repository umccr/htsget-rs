use async_trait::async_trait;

use crate::htsget::{Format, Query, Response, Result};

/// Trait representing a search for either `reads` or `variants` in the HtsGet specification.
#[async_trait]
pub trait HtsGet {
  async fn search(&self, query: Query) -> Result<Response>;
  fn get_supported_formats(&self) -> Vec<Format>;
  fn are_field_parameters_effective(&self) -> bool;
  fn are_tag_parameters_effective(&self) -> bool;
}
