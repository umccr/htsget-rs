//! Configuration related to CORS.
//!

use std::fmt::{Display, Formatter};
use std::str::FromStr;

use http::header::{HeaderName, HeaderValue as HeaderValueInner, InvalidHeaderValue};
use http::Method;
use serde::de::Error;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::types::TaggedTypeAll;

/// The maximum default amount of time a CORS request can be cached for in seconds.
/// Defaults to 30 days.
const CORS_MAX_AGE: usize = 2592000;

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
pub enum AllowType<T, Tagged = TaggedAllowTypes> {
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
  /// Get the inner header value.
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct CorsConfig {
  allow_credentials: bool,
  allow_origins: AllowType<HeaderValue>,
  allow_headers: AllowType<HeaderName>,
  allow_methods: AllowType<Method>,
  max_age: usize,
  expose_headers: AllowType<HeaderName, TaggedTypeAll>,
}

impl CorsConfig {
  /// Create new cors config.
  pub fn new(
    allow_credentials: bool,
    allow_origins: AllowType<HeaderValue, TaggedAllowTypes>,
    allow_headers: AllowType<HeaderName>,
    allow_methods: AllowType<Method>,
    max_age: usize,
    expose_headers: AllowType<HeaderName, TaggedTypeAll>,
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
  pub fn expose_headers(&self) -> &AllowType<HeaderName, TaggedTypeAll> {
    &self.expose_headers
  }
}

impl Default for CorsConfig {
  fn default() -> Self {
    Self {
      allow_credentials: false,
      allow_origins: AllowType::Tagged(TaggedAllowTypes::Mirror),
      allow_headers: AllowType::Tagged(TaggedAllowTypes::Mirror),
      allow_methods: AllowType::Tagged(TaggedAllowTypes::Mirror),
      max_age: CORS_MAX_AGE,
      expose_headers: AllowType::Tagged(TaggedTypeAll::All),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::tests::test_serialize_and_deserialize;
  use crate::config::Config;
  use http::Method;
  use toml::de::Error;

  #[test]
  fn unit_variant_any_allow_type() {
    test_serialize_and_deserialize(
      "allow_methods = \"All\"",
      CorsConfig {
        allow_methods: AllowType::Tagged(TaggedAllowTypes::All),
        ..Default::default()
      },
      |result| result,
    );
  }

  #[test]
  fn unit_variant_mirror_allow_type() {
    test_serialize_and_deserialize(
      "allow_origins = \"Mirror\"",
      CorsConfig {
        allow_origins: AllowType::Tagged(TaggedAllowTypes::Mirror),
        ..Default::default()
      },
      |result| result,
    );
  }

  #[test]
  fn list_allow_type() {
    test_serialize_and_deserialize(
      "allow_methods = [\"GET\"]",
      CorsConfig {
        allow_methods: AllowType::List(vec![Method::GET]),
        ..Default::default()
      },
      |result| result,
    );
  }

  #[test]
  fn tagged_any_allow_type() {
    test_serialize_and_deserialize(
      "expose_headers = \"All\"",
      CorsConfig {
        expose_headers: AllowType::Tagged(TaggedTypeAll::All),
        ..Default::default()
      },
      |result| result,
    );
  }

  #[test]
  fn cors_config() {
    test_serialize_and_deserialize(
      r#"
      ticket_server.cors.allow_credentials = false
      ticket_server.cors.allow_origins = "Mirror"
      ticket_server.cors.allow_headers = "All"
      data_server.cors.allow_methods = ["GET", "POST"]
      data_server.cors.max_age = 86400
      data_server.cors.expose_headers = []
      "#,
      (
        false,
        AllowType::Tagged(TaggedAllowTypes::Mirror),
        AllowType::Tagged(TaggedAllowTypes::All),
        AllowType::List(vec!["GET".parse().unwrap(), "POST".parse().unwrap()]),
        86400,
        AllowType::List(vec![]),
      ),
      |result: Config| {
        let ticket_cors = result.ticket_server().cors();
        let data_cors = result.data_server().as_data_server_config().unwrap().cors();

        (
          ticket_cors.allow_credentials,
          ticket_cors.allow_origins.clone(),
          ticket_cors.allow_headers.clone(),
          data_cors.allow_methods.clone(),
          data_cors.max_age,
          data_cors.expose_headers.clone(),
        )
      },
    );
  }

  #[test]
  fn tagged_any_allow_type_err_on_mirror() {
    let allow_type_method = "expose_headers = \"Mirror\"";
    let config: Result<CorsConfig, Error> = toml::from_str(allow_type_method);
    assert!(config.is_err());
  }
}
