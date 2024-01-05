use http::Uri;
use serde::{Deserialize, Serialize};

use crate::storage::url::{default_url, ValidatedUrl};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Endpoints {
  head: ValidatedUrl,
  index: ValidatedUrl,
  file: ValidatedUrl,
}

impl Default for Endpoints {
  fn default() -> Self {
    Self {
      head: default_url(),
      index: default_url(),
      file: default_url(),
    }
  }
}

impl Endpoints {
  /// Construct a new endpoints config.
  pub fn new(head: ValidatedUrl, index: ValidatedUrl, file: ValidatedUrl) -> Self {
    Self { head, index, file }
  }

  /// Get the head endpoint.
  pub fn head(&self) -> &Uri {
    &self.head.0.inner
  }

  /// Get the index endpoint.
  pub fn index(&self) -> &Uri {
    &self.index.0.inner
  }

  /// Get the file endpoint.
  pub fn file(&self) -> &Uri {
    &self.file.0.inner
  }
}
