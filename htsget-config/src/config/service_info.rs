//! Service info configuration.
//!

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Service info config.
#[derive(Serialize, Debug, Clone, Default)]
#[serde(default)]
pub struct ServiceInfo {
  fields: HashMap<String, Value>,
}

impl ServiceInfo {
  /// Create a service info.
  pub fn new(fields: HashMap<String, Value>) -> Self {
    Self { fields }
  }

  /// Get the inner value.
  pub fn into_inner(self) -> HashMap<String, Value> {
    self.fields
  }
}

impl<'de> Deserialize<'de> for ServiceInfo {
  fn deserialize<D>(deserializer: D) -> Result<ServiceInfo, D::Error>
  where
    D: Deserializer<'de>,
  {
    let fields: HashMap<String, Value> = HashMap::<String, Value>::deserialize(deserializer)?
      .into_iter()
      .map(|(key, value)| (key.to_lowercase(), value))
      .collect();

    let err_msg = |invalid_key| format!("reserved service info field `{}`", invalid_key);

    if fields.contains_key("type") {
      return Err(Error::custom(err_msg("type")));
    }

    if fields.contains_key("htsget") {
      return Err(Error::custom(err_msg("htsget")));
    }

    Ok(ServiceInfo::new(fields))
  }
}
