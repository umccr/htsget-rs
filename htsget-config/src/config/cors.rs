use std::fmt::{Display, Formatter};
use std::str::FromStr;
use http::header::{HeaderName, InvalidHeaderValue, HeaderValue as HeaderValueInner};
use http::Method;
use serde::{Deserialize, Serialize, Deserializer, Serializer};
use serde::de::Error;
use serde::ser::SerializeSeq;
use crate::config::default_server_origin;

/// The maximum default amount of time a CORS request can be cached for in seconds.
const CORS_MAX_AGE: usize = 86400;

/// Tagged allow headers for cors config. Either Mirror or Any.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TaggedAllowTypes {
    #[serde(alias = "mirror", alias = "MIRROR")]
    Mirror,
    #[serde(alias = "any", alias = "ANY")]
    Any
}

/// Tagged allow headers for cors config. Either Mirror or Any.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TaggedAnyAllowType {
    #[serde(alias = "any", alias = "ANY")]
    Any
}

/// Allowed header for cors config. Any allows all headers by sending a wildcard,
/// and mirror allows all headers by mirroring the received headers.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum AllowType<T, Tagged = TaggedAllowTypes> {
    Tagged(Tagged),
    #[serde(bound(serialize = "T: Display", deserialize = "T: FromStr, T::Err: Display"))]
    #[serde(serialize_with = "serialize_allow_types", deserialize_with = "deserialize_allow_types")]
    List(Vec<T>)
}

fn serialize_allow_types<S, T>(names: &Vec<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer
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
        D: Deserializer<'de>
{
    let names: Vec<String> = Deserialize::deserialize(deserializer)?;
    names.into_iter().map(|name| T::from_str(&name).map_err(Error::custom)).collect()
}

#[derive(Debug, Clone)]
pub struct HeaderValue(HeaderValueInner);

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
    cors_allow_credentials: bool,
    cors_allow_origins: AllowType<HeaderValue>,
    cors_allow_headers: AllowType<HeaderName>,
    cors_allow_methods: AllowType<Method>,
    cors_max_age: usize,
    cors_expose_headers: AllowType<HeaderName, TaggedAnyAllowType>,
}

impl CorsConfig {
    pub fn allow_credentials(&self) -> bool {
        self.cors_allow_credentials
    }

    pub fn allow_origins(&self) -> &AllowType<HeaderValue> {
        &self.cors_allow_origins
    }

    pub fn allow_headers(&self) -> &AllowType<HeaderName> {
        &self.cors_allow_headers
    }

    pub fn allow_methods(&self) -> &AllowType<Method> {
        &self.cors_allow_methods
    }

    pub fn max_age(&self) -> usize {
        self.cors_max_age
    }

    pub fn expose_headers(&self) -> &AllowType<HeaderName, TaggedAnyAllowType> {
        &self.cors_expose_headers
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            cors_allow_credentials: false,
            cors_allow_origins: AllowType::List(vec![HeaderValue(HeaderValueInner::from_static(default_server_origin()))]),
            cors_allow_headers: AllowType::Tagged(TaggedAllowTypes::Mirror),
            cors_allow_methods: AllowType::Tagged(TaggedAllowTypes::Mirror),
            cors_max_age: CORS_MAX_AGE,
            cors_expose_headers: AllowType::List(vec![]),
        }
    }
}