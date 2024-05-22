use std::fmt::{Display, Formatter};
use std::str::FromStr;

use http::header::{HeaderName, HeaderValue as HeaderValueInner, InvalidHeaderValue};
use http::Method;
use serde::de::Error;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::config::default_server_origin;
use crate::types::TaggedTypeAll;

/// The maximum default amount of time a CORS request can be cached for in seconds.
const CORS_MAX_AGE: usize = 86400;

/// Tagged allow headers for cors config, either Mirror or Any.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TaggedAllowTypes {
  #[serde(alias = "mirror", alias = "MIRROR")]
  Mirror,
  #[serde(alias = "all", alias = "ALL")]
  All,
}

/// Allowed type for cors config which is used to configure cors behaviour.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum AllowType<T, Tagged = TaggedTypeAll> {
  Tagged(Tagged),
  #[serde(bound(serialize = "T: Display", deserialize = "T: FromStr, T::Err: Display"))]
  #[serde(
    serialize_with = "serialize_allow_types",
    deserialize_with = "deserialize_allow_types"
  )]
  List(Vec<T>),
}

impl<T, Tagged> AllowType<T, Tagged> {
  /// Apply a function to the builder when the type is a List.
  pub fn apply_list<F, U>(&self, func: F, builder: U) -> U
  where
    F: FnOnce(U, &Vec<T>) -> U,
  {
    if let Self::List(list) = self {
      func(builder, list)
    } else {
      builder
    }
  }

  /// Apply a function to the builder when the type is a List returning a Result.
  pub fn try_apply_list<F, U, E>(&self, func: F, builder: U) -> Result<U, E>
  where
    F: FnOnce(U, &Vec<T>) -> Result<U, E>,
  {
    if let Self::List(list) = self {
      func(builder, list)
    } else {
      Ok(builder)
    }
  }

  /// Apply a function to the builder when the type is tagged.
  pub fn apply_tagged<F, U>(&self, func: F, builder: U, tagged_type: &Tagged) -> U
  where
    F: FnOnce(U) -> U,
    Tagged: Eq,
  {
    if let Self::Tagged(tagged) = self {
      if tagged == tagged_type {
        return func(builder);
      }
    }

    builder
  }
}

impl<T> AllowType<T, TaggedAllowTypes> {
  /// Apply a function to the builder when the type is Mirror.
  pub fn apply_mirror<F, U>(&self, func: F, builder: U) -> U
  where
    F: FnOnce(U) -> U,
  {
    self.apply_tagged(func, builder, &TaggedAllowTypes::Mirror)
  }

  /// Apply a function to the builder when the type is Any.
  pub fn apply_any<F, U>(&self, func: F, builder: U) -> U
  where
    F: FnOnce(U) -> U,
  {
    self.apply_tagged(func, builder, &TaggedAllowTypes::All)
  }
}

impl<T> AllowType<T, TaggedTypeAll> {
  /// Apply a function to the builder when the type is Any.
  pub fn apply_any<F, U>(&self, func: F, builder: U) -> U
  where
    F: FnOnce(U) -> U,
  {
    self.apply_tagged(func, builder, &TaggedTypeAll::All)
  }
}

fn serialize_allow_types<S, T>(names: &[T], serializer: S) -> Result<S::Ok, S::Error>
where
  T: Display,
  S: Serializer,
{
  let mut sequence = serializer.serialize_seq(Some(names.len()))?;
  for element in names.iter().map(|name| format!("{name}")) {
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

/// A wrapper around a http HeaderValue which is used to implement FromStr and Display.
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

/// Cors configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct CorsConfig {
  allow_credentials: bool,
  allow_origins: AllowType<HeaderValue, TaggedAllowTypes>,
  allow_headers: AllowType<HeaderName>,
  allow_methods: AllowType<Method>,
  max_age: usize,
  expose_headers: AllowType<HeaderName>,
}

impl CorsConfig {
  /// Create new cors config.
  pub fn new(
    allow_credentials: bool,
    allow_origins: AllowType<HeaderValue, TaggedAllowTypes>,
    allow_headers: AllowType<HeaderName>,
    allow_methods: AllowType<Method>,
    max_age: usize,
    expose_headers: AllowType<HeaderName>,
  ) -> Self {
    Self {
      allow_credentials,
      allow_origins,
      allow_headers,
      allow_methods,
      max_age,
      expose_headers,
    }
  }

  /// Get allow credentials.
  pub fn allow_credentials(&self) -> bool {
    self.allow_credentials
  }

  /// Get allow origins.
  pub fn allow_origins(&self) -> &AllowType<HeaderValue, TaggedAllowTypes> {
    &self.allow_origins
  }

  /// Get allow headers.
  pub fn allow_headers(&self) -> &AllowType<HeaderName> {
    &self.allow_headers
  }

  /// Get allow methods.
  pub fn allow_methods(&self) -> &AllowType<Method> {
    &self.allow_methods
  }

  /// Get max age.
  pub fn max_age(&self) -> usize {
    self.max_age
  }

  /// Get expose headers.
  pub fn expose_headers(&self) -> &AllowType<HeaderName> {
    &self.expose_headers
  }
}

impl Default for CorsConfig {
  fn default() -> Self {
    Self {
      allow_credentials: false,
      allow_origins: AllowType::List(vec![HeaderValue(HeaderValueInner::from_static(
        default_server_origin(),
      ))]),
      allow_headers: AllowType::Tagged(TaggedTypeAll::All),
      allow_methods: AllowType::Tagged(TaggedTypeAll::All),
      max_age: CORS_MAX_AGE,
      expose_headers: AllowType::List(vec![]),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::fmt::Debug;

  use http::Method;
  use toml::de::Error;

  use super::*;

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
      "allow_methods = \"All\"",
      &AllowType::Tagged(TaggedTypeAll::All),
      |config| config.allow_methods(),
    );
  }

  #[test]
  fn unit_variant_mirror_allow_type() {
    test_cors_config(
      "allow_origins = \"Mirror\"",
      &AllowType::Tagged(TaggedAllowTypes::Mirror),
      |config| config.allow_origins(),
    );
  }

  #[test]
  fn list_allow_type() {
    test_cors_config(
      "allow_methods = [\"GET\"]",
      &AllowType::List(vec![Method::GET]),
      |config| config.allow_methods(),
    );
  }

  #[test]
  fn tagged_any_allow_type() {
    test_cors_config(
      "expose_headers = \"All\"",
      &AllowType::Tagged(TaggedTypeAll::All),
      |config| config.expose_headers(),
    );
  }

  #[test]
  fn tagged_any_allow_type_err_on_mirror() {
    let allow_type_method = "expose_headers = \"Mirror\"";
    let config: Result<CorsConfig, Error> = toml::from_str(allow_type_method);
    assert!(config.is_err());
  }
}
