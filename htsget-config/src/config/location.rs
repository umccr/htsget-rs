//! Storage location configuration.
//!

use crate::config::advanced::regex_location::RegexLocation;
use crate::error::Result;
use crate::storage::Backend;
use serde::{Deserialize, Serialize};

/// The locations of data.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Locations(Vec<LocationEither>);

impl Locations {
  /// Create new locations.
  pub fn new(locations: Vec<LocationEither>) -> Self {
    Self(locations)
  }

  /// Get locations as a slice of `LocationEither`.
  pub fn as_slice(&self) -> &[LocationEither] {
    self.0.as_slice()
  }

  /// Get locations as an owned vector of `LocationEither`.
  pub fn into_inner(self) -> Vec<LocationEither> {
    self.0
  }

  /// Get locations as a mutable slice of `LocationEither`.
  pub fn as_mut_slice(&mut self) -> &mut [LocationEither] {
    self.0.as_mut_slice()
  }
}

impl Default for Locations {
  fn default() -> Self {
    Self(vec![Default::default()])
  }
}

/// Either simple or regex based location.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum LocationEither {
  Regex(RegexLocation),
}

impl LocationEither {
  /// Get the storage backend.
  pub fn backend(&self) -> &Backend {
    match self {
      LocationEither::Regex(regex_location) => regex_location.backend(),
    }
  }

  /// Get the regex location variant, returning an error otherwise.
  pub fn as_regex(&self) -> Result<&RegexLocation> {
    let LocationEither::Regex(regex) = self;
    Ok(regex)
  }
}

impl Default for LocationEither {
  fn default() -> Self {
    Self::Regex(Default::default())
  }
}
