use crate::config::default_server_origin;
use http::header::{HeaderName, HeaderValue as HeaderValueInner, InvalidHeaderValue};
use http::Method;
use serde::de::Error;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// The maximum default amount of time a CORS request can be cached for in seconds.
const CORS_MAX_AGE: usize = 86400;

/// Tagged allow headers for cors config. Either Mirror or Any.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TaggedAllowTypes {
  #[serde(alias = "mirror", alias = "MIRROR")]
  Mirror,
  #[serde(alias = "any", alias = "ANY")]
  Any,
}

/// Tagged allow headers for cors config. Either Mirror or Any.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TaggedAnyAllowType {
  #[serde(alias = "any", alias = "ANY")]
  Any,
}

/// Allowed header for cors config. Any allows all headers by sending a wildcard,
/// and mirror allows all headers by mirroring the received headers.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum AllowType<T, Tagged = TaggedAllowTypes> {
  Tagged(Tagged),
  #[serde(bound(serialize = "T: Display", deserialize = "T: FromStr, T::Err: Display"))]
  #[serde(
    serialize_with = "serialize_allow_types",
    deserialize_with = "deserialize_allow_types"
  )]
  List(Vec<T>),
}

fn serialize_allow_types<S, T>(names: &Vec<T>, serializer: S) -> Result<S::Ok, S::Error>
where
  T: Display,
  S: Serializer,
{
  let mut sequence = serializer.serialize_seq(Some(names.len()))?;
  for element in names.iter().map(|name| format!("{}", name)) {
    sequence.serialize_element(&element)?;
  }
  sequence.end()
}

fn deserialize_allow_types<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
  T: FromStr,
  T::Err: Display,
  D: Deserializer<'de>,
{
  let names: Vec<String> = Deserialize::deserialize(deserializer)?;
  names
    .into_iter()
    .map(|name| T::from_str(&name).map_err(Error::custom))
    .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderValue(HeaderValueInner);

impl HeaderValue {
  pub fn into_inner(self) -> HeaderValueInner {
    self.0
  }
}

impl FromStr for HeaderValue {
  type Err = InvalidHeaderValue;

  fn from_str(header: &str) -> Result<Self, Self::Err> {
    Ok(HeaderValue(HeaderValueInner::from_str(header)?))
  }
}

impl Display for HeaderValue {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    f.write_str(&String::from_utf8_lossy(self.0.as_ref()))
  }
}

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct CorsConfig {
  allow_credentials: bool,
  allow_origins: AllowType<HeaderValue>,
  allow_headers: AllowType<HeaderName>,
  allow_methods: AllowType<Method>,
  max_age: usize,
  expose_headers: AllowType<HeaderName, TaggedAnyAllowType>,
}

impl CorsConfig {
  pub fn allow_credentials(&self) -> bool {
    self.allow_credentials
  }

  pub fn allow_origins(&self) -> &AllowType<HeaderValue> {
    &self.allow_origins
  }

  pub fn allow_headers(&self) -> &AllowType<HeaderName> {
    &self.allow_headers
  }

  pub fn allow_methods(&self) -> &AllowType<Method> {
    &self.allow_methods
  }

  pub fn max_age(&self) -> usize {
    self.max_age
  }

  pub fn expose_headers(&self) -> &AllowType<HeaderName, TaggedAnyAllowType> {
    &self.expose_headers
  }

  pub fn set_allow_credentials(&mut self, allow_credentials: bool) {
    self.allow_credentials = allow_credentials;
  }

  pub fn set_allow_origins(&mut self, allow_origins: AllowType<HeaderValue>) {
    self.allow_origins = allow_origins;
  }

  pub fn set_allow_headers(&mut self, allow_headers: AllowType<HeaderName>) {
    self.allow_headers = allow_headers;
  }

  pub fn set_allow_methods(&mut self, allow_methods: AllowType<Method>) {
    self.allow_methods = allow_methods;
  }

  pub fn set_max_age(&mut self, max_age: usize) {
    self.max_age = max_age;
  }

  pub fn set_expose_headers(&mut self, expose_headers: AllowType<HeaderName, TaggedAnyAllowType>) {
    self.expose_headers = expose_headers;
  }
}

impl Default for CorsConfig {
  fn default() -> Self {
    Self {
      allow_credentials: false,
      allow_origins: AllowType::List(vec![HeaderValue(HeaderValueInner::from_static(
        default_server_origin(),
      ))]),
      allow_headers: AllowType::Tagged(TaggedAllowTypes::Mirror),
      allow_methods: AllowType::Tagged(TaggedAllowTypes::Mirror),
      max_age: CORS_MAX_AGE,
      expose_headers: AllowType::List(vec![]),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use http::Method;
  use std::fmt::Debug;
  use toml::de::Error;

  fn test_cors_config<T, F>(input: &str, expected: &T, get_result: F)
  where
    F: Fn(&CorsConfig) -> &T,
    T: Debug + Eq,
  {
    let config: CorsConfig = toml::from_str(input).unwrap();
    assert_eq!(expected, get_result(&config));

    let serialized = toml::to_string(&config).unwrap();
    let deserialized = toml::from_str(&serialized).unwrap();
    assert_eq!(expected, get_result(&deserialized));
  }

  #[test]
  fn unit_variant_any_allow_type() {
    test_cors_config(
      "cors_allow_methods = \"Any\"",
      &AllowType::Tagged(TaggedAllowTypes::Any),
      |config| config.allow_methods(),
    );
  }

  #[test]
  fn unit_variant_mirror_allow_type() {
    test_cors_config(
      "cors_allow_methods = \"Mirror\"",
      &AllowType::Tagged(TaggedAllowTypes::Mirror),
      |config| config.allow_methods(),
    );
  }

  #[test]
  fn list_allow_type() {
    test_cors_config(
      "cors_allow_methods = [\"GET\"]",
      &AllowType::List(vec![Method::GET]),
      |config| config.allow_methods(),
    );
  }

  #[test]
  fn tagged_any_allow_type() {
    test_cors_config(
      "cors_expose_headers = \"Any\"",
      &AllowType::Tagged(TaggedAnyAllowType::Any),
      |config| config.expose_headers(),
    );
  }

  #[test]
  fn tagged_any_allow_type_err_on_mirror() {
    let allow_type_method = "cors_expose_headers = \"Mirror\"";
    let config: Result<CorsConfig, Error> = toml::from_str(allow_type_method);
    assert!(matches!(config, Err(_)));
  }
}
