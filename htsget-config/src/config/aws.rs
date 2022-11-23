use serde;
use serde::Deserialize;

/// Configuration for the htsget server.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct AwsS3DataServer {
  pub bucket: String,
}