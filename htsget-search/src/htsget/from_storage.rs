//! Module providing an implementation of the [HtsGet] trait using a [Storage].
//!

use crate::{
  htsget::bam::BamSearch,
  htsget::{Format, HtsGet, HtsGetError, Query, Response, Result},
  storage::Storage,
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
      Some(Format::BAM) | None => BamSearch::new(&self.storage).search(query),
      Some(format) => Err(HtsGetError::unsupported_format(format)),
    }
  }
}

impl<S> HtsGetFromStorage<S> {
  pub fn new(storage: S) -> Self {
    Self { storage }
  }
}

#[cfg(test)]
mod tests {

  use crate::storage::local::LocalStorage;

  use super::*;

  #[test]
  fn search_() {
    // TODO determine root path through cargo env vars
    let storage = LocalStorage::new("../data");
    let htsget = HtsGetFromStorage::new(storage);
  }
}
