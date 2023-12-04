use crate::storage::url::{default_url, ValidatedUrl};
use http::Uri;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Endpoints {
  head: ValidatedUrl,
  index: ValidatedUrl,
  file: ValidatedUrl,
  #[cfg(feature = "crypt4gh")]
  public_key: Option<ValidatedUrl>,
}

impl Default for Endpoints {
  fn default() -> Self {
    Self {
      head: default_url(),
      index: default_url(),
      file: default_url(),
      #[cfg(feature = "crypt4gh")]
      public_key: None,
    }
  }
}

impl Endpoints {
  /// Construct a new endpoints config.
  pub fn new(head: ValidatedUrl, index: ValidatedUrl, file: ValidatedUrl) -> Self {
    Self {
      head,
      index,
      file,
      #[cfg(feature = "crypt4gh")]
      public_key: None,
    }
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

  /// Set the endpoint public key.
  #[cfg(feature = "crypt4gh")]
  pub fn with_public_key(mut self, endpoint_public_key: ValidatedUrl) -> Self {
    self.public_key = Some(endpoint_public_key);
    self
  }

  /// Get the public key endpoint.
  #[cfg(feature = "crypt4gh")]
  pub fn public_key(&self) -> Option<&Uri> {
    self.public_key.as_ref().map(|url| &url.0.inner)
  }
}
