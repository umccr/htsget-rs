//! Service info configuration.
//!

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Service info config.
#[derive(Serialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ServiceInfo(HashMap<String, Value>);

impl ServiceInfo {
  /// Create a service info.
  pub fn new(fields: HashMap<String, Value>) -> Self {
    Self(fields)
  }

  /// Get the inner value.
  pub fn into_inner(self) -> HashMap<String, Value> {
    self.0
  }
}

impl AsRef<HashMap<String, Value>> for ServiceInfo {
  fn as_ref(&self) -> &HashMap<String, Value> {
    &self.0
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::tests::test_serialize_and_deserialize;
  use crate::config::Config;
  use serde_json::json;

  #[test]
  fn service_info() {
    test_serialize_and_deserialize(
      r#"
      service_info.environment = "dev"
      service_info.organization = { name = "name", url = "https://example.com/" }
      "#,
      HashMap::from_iter(vec![
        ("environment".to_string(), json!("dev")),
        (
          "organization".to_string(),
          json!({ "name": "name", "url": "https://example.com/" }),
        ),
      ]),
      |result: Config| result.service_info.0,
    );
  }
}
