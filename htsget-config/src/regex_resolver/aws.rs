use serde;
use serde::{Deserialize, Serialize};

/// Configuration for the htsget server.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(default)]
pub struct S3Resolver {
  pub bucket: String,
  pub cors_allow_credentials: bool,
  pub cors_allow_origin: String,
}
