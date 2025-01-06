//! Service info configuration.
//!

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Formats the package info. This uses `CARGO_PKG_VERSION` and `CARGO_PKG_NAME` to
/// format the package info of the dependent crate. A macro allows the calling code
/// to use its own version and name. For example, instead of printing
/// `htsget-config/x.y.z`, this allows printing `htsget-axum/x.y.z.`.
#[macro_export]
macro_rules! package_info {
  () => {
    format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
  };
}

/// Formats the repository url using `CARGO_PKG_REPOSITORY`. A macro allows calling
/// code to use its own repository url.
#[macro_export]
macro_rules! repository {
  () => {
    format!(env!("CARGO_PKG_REPOSITORY"))
  };
}

/// Formats the description using `CARGO_PKG_DESCRIPTION`. A macro allows calling
/// code to use its own description.
#[macro_export]
macro_rules! description {
  () => {
    format!(env!("CARGO_PKG_DESCRIPTION"))
  };
}

/// Service info config.
#[derive(Serialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ServiceInfo(HashMap<String, Value>);

impl ServiceInfo {
  /// Create a service info.
  pub fn new(fields: HashMap<String, Value>) -> Self {
    Self(fields)
  }

  /// Insert the value if it does not already exist.
  pub fn entry_or_insert(&mut self, key: String, value: Value) -> &mut Value {
    self.0.entry(key).or_insert(value)
  }

  /// Insert the package info if it doesn't already exist.
  pub fn insert_package_info(&mut self, info: String) {
    self.entry_or_insert("packageInfo".to_string(), json!(info));
  }

  /// Insert the description if it doesn't already exist.
  pub fn insert_description(&mut self, description: String) {
    self.entry_or_insert("description".to_string(), json!(description));
  }

  /// Insert the repository if it doesn't already exist.
  pub fn insert_repository(&mut self, repository: String) {
    self.entry_or_insert("repository".to_string(), json!(repository));
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
