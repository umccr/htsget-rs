//! Storage location configuration.
//!

use crate::config::advanced::regex_location::RegexLocation;
use crate::error::{Error::ParseError, Result};
use crate::storage;
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use crate::storage::file::default_authority;
use crate::storage::Backend;
use crate::types::Scheme;
use cfg_if::cfg_if;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use std::result;
#[cfg(feature = "url")]
use {crate::config::advanced::url::Url, crate::error, http::Uri};

/// The locations of data.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields, from = "LocationsOneOrMany")]
pub struct Locations(Vec<LocationEither>);

impl Locations {
  /// Create new locations.
  pub fn new(locations: Vec<LocationEither>) -> Self {
    Self(locations)
  }

  /// Get locations as a slice of `LocationEither`.
  pub fn as_slice(&self) -> &[LocationEither] {
    self.0.as_slice()
  }

  /// Get locations as an owned vector of `LocationEither`.
  pub fn into_inner(self) -> Vec<LocationEither> {
    self.0
  }

  /// Get locations as a mutable slice of `LocationEither`.
  pub fn as_mut_slice(&mut self) -> &mut [LocationEither] {
    self.0.as_mut_slice()
  }
}

impl Default for Locations {
  fn default() -> Self {
    Self(vec![Default::default()])
  }
}

/// Either simple or regex based location
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged, deny_unknown_fields)]
pub enum LocationEither {
  Simple(Box<Location>),
  Regex(Box<RegexLocation>),
}

impl LocationEither {
  /// Get the storage backend.
  pub fn backend(&self) -> &Backend {
    match self {
      LocationEither::Simple(location) => location.backend(),
      LocationEither::Regex(regex_location) => regex_location.backend(),
    }
  }

  /// Get the simple location variant, returning an error otherwise.
  pub fn as_simple(&self) -> Result<&Location> {
    if let LocationEither::Simple(simple) = self {
      Ok(simple)
    } else {
      Err(ParseError("not a `Simple` variant".to_string()))
    }
  }

  /// Get the regex location variant, returning an error otherwise.
  pub fn as_regex(&self) -> Result<&RegexLocation> {
    if let LocationEither::Regex(regex) = self {
      Ok(regex)
    } else {
      Err(ParseError("not a `Regex` variant".to_string()))
    }
  }
}

impl Default for LocationEither {
  fn default() -> Self {
    Self::Simple(Default::default())
  }
}

/// Location config.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default, from = "LocationWrapper", deny_unknown_fields)]
pub struct Location {
  backend: Backend,
  prefix: String,
}

impl Location {
  /// Create a new location.
  pub fn new(backend: Backend, prefix: String) -> Self {
    Self { backend, prefix }
  }

  /// Get the storage backend.
  pub fn backend(&self) -> &Backend {
    &self.backend
  }

  /// Get the prefix.
  pub fn prefix(&self) -> &str {
    &self.prefix
  }
}

/// Either a single or many locations
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged, deny_unknown_fields)]
enum LocationsOneOrMany {
  Many(Vec<LocationEither>),
  One(Box<LocationEither>),
}

impl From<LocationsOneOrMany> for Locations {
  fn from(locations: LocationsOneOrMany) -> Self {
    match locations {
      LocationsOneOrMany::One(location) => Self(vec![*location]),
      LocationsOneOrMany::Many(locations) => Self(locations),
    }
  }
}

/// Deserialize into a string location that also supports setting additional fields
/// for the backend.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default, deny_unknown_fields)]
struct ExtendedLocation {
  location: StringLocation,
  #[cfg(feature = "experimental")]
  #[serde(skip_serializing)]
  keys: Option<C4GHKeys>,
}

/// Deserialize the location from a string with a protocol.
#[derive(Serialize, Debug, Clone, Default)]
#[serde(default, deny_unknown_fields)]
struct StringLocation {
  backend: Backend,
  prefix: String,
}

/// Deserialize the location from a map with regular field and values.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default, deny_unknown_fields)]
struct MapLocation {
  backend: Backend,
  prefix: String,
}

/// A wrapper around location deserialization that can deserialize either a string
/// or a map. This is required so that default values behave correctly when deserializing
/// the `Location`. For example, if a location string isn't specified, the `Deserialize`
/// implementation for `StringLocation` can't account for this as it gets passed default values
/// which contain map elements. This wrapper allows deserializing using regular semantics by
/// falling back to the regular `MapLocation` derived deserializer. The reason there needs to be a
/// `StringLocation` and `MapLocation` type is so that `Location` can be deserialized using the
/// `from` attribute without recursion.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged, deny_unknown_fields)]
enum LocationWrapper {
  String(StringLocation),
  Map(MapLocation),
  Extended(ExtendedLocation),
}

impl From<LocationWrapper> for Location {
  fn from(location: LocationWrapper) -> Self {
    match location {
      LocationWrapper::String(location) => Location::new(location.backend, location.prefix),
      LocationWrapper::Map(location) => Location::new(location.backend, location.prefix),
      LocationWrapper::Extended(location) => {
        cfg_if! {
          if #[cfg(feature = "experimental")] {
            let mut location = location;
            location.location.backend.set_keys(location.keys);
            Location::new(location.location.backend, location.location.prefix)
          } else {
            Location::new(location.location.backend, location.location.prefix)
          }
        }
      }
    }
  }
}

impl From<Location> for LocationEither {
  fn from(location: Location) -> Self {
    Self::Simple(Box::new(location))
  }
}

impl<'de> Deserialize<'de> for StringLocation {
  fn deserialize<D>(deserializer: D) -> result::Result<StringLocation, D::Error>
  where
    D: Deserializer<'de>,
  {
    let split = |s: &str| {
      let (s1, s2) = if let Some(split) = s.split_once("/").map(|(s1, s2)| {
        (
          s1.to_string(),
          s2.strip_suffix('/').unwrap_or(s2).to_string(),
        )
      }) {
        split
      } else {
        (s.to_string(), "".to_string())
      };

      if s1.is_empty() {
        Err(Error::custom("cannot have empty location"))
      } else {
        Ok((s1, s2))
      }
    };

    let s = String::deserialize(deserializer)?.to_lowercase();

    if let Some(s) = s.strip_prefix("file://") {
      let (path, prefix) = split(s)?;
      return Ok(StringLocation {
        backend: Backend::File(storage::file::File::new(
          Scheme::Http,
          default_authority(),
          path.to_string(),
        )),
        prefix,
      });
    }

    #[cfg(feature = "aws")]
    if let Some(s) = s.strip_prefix("s3://") {
      let (bucket, prefix) = split(s)?;
      return Ok(StringLocation {
        backend: Backend::S3(storage::s3::S3::new(bucket.to_string(), None, false)),
        prefix,
      });
    }

    #[cfg(feature = "url")]
    if let Some(s_stripped) = s
      .strip_prefix("http://")
      .or_else(|| s.strip_prefix("https://"))
    {
      let (mut uri, prefix) = split(s_stripped)?;

      if s.starts_with("http://") {
        uri = format!("http://{uri}");
      }
      if s.starts_with("https://") {
        uri = format!("https://{uri}");
      }

      let uri: Uri = uri.parse().map_err(Error::custom)?;
      let url = Url::new(uri.clone(), Some(uri), true, vec![], Default::default())
        .try_into()
        .map_err(|err: error::Error| Error::custom(err.to_string()))?;

      return Ok(StringLocation {
        backend: Backend::Url(url),
        prefix,
      });
    }

    Err(Error::custom(
      "expected file://, s3://, http:// or https:// scheme",
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::tests::test_serialize_and_deserialize;
  use crate::config::Config;

  #[test]
  fn location_single() {
    test_serialize_and_deserialize(
      r#"
      locations = "file://path/prefix1"
      "#,
      ("path".to_string(), "prefix1".to_string()),
      |result: Config| assert_file_location(result),
    );
    test_serialize_and_deserialize(
      r#"
      locations = "file://path/prefix1/"
      "#,
      ("path".to_string(), "prefix1".to_string()),
      |result: Config| assert_file_location(result),
    );
  }

  #[test]
  fn location_no_prefix() {
    test_serialize_and_deserialize(
      r#"
      locations = "file://path"
      "#,
      ("path".to_string(), "".to_string()),
      |result: Config| assert_file_location(result),
    );
    test_serialize_and_deserialize(
      r#"
      locations = "file://path/"
      "#,
      ("path".to_string(), "".to_string()),
      |result: Config| assert_file_location(result),
    );
  }

  #[test]
  fn location_file() {
    test_serialize_and_deserialize(
      r#"
      locations = [ "file://path/prefix1", "file://path/prefix2" ]
      "#,
      (
        "path".to_string(),
        "prefix1".to_string(),
        "path".to_string(),
        "prefix2".to_string(),
      ),
      |result: Config| {
        let result = result.locations.0;
        assert_eq!(result.len(), 2);
        if let (LocationEither::Simple(location1), LocationEither::Simple(location2)) =
          (result.first().unwrap(), result.get(1).unwrap())
        {
          let file1 = location1.backend().as_file().unwrap();
          let file2 = location2.backend().as_file().unwrap();

          return (
            file1.local_path().to_string(),
            location1.prefix().to_string(),
            file2.local_path().to_string(),
            location2.prefix().to_string(),
          );
        }

        panic!();
      },
    );
  }

  #[cfg(feature = "aws")]
  #[test]
  fn location_s3() {
    test_serialize_and_deserialize(
      r#"
      locations = [ "s3://bucket/prefix1", "s3://bucket/prefix2" ]
      "#,
      (
        "bucket".to_string(),
        "prefix1".to_string(),
        "bucket".to_string(),
        "prefix2".to_string(),
      ),
      |result: Config| {
        let result = result.locations.0;
        assert_eq!(result.len(), 2);
        if let (LocationEither::Simple(location1), LocationEither::Simple(location2)) =
          (result.first().unwrap(), result.get(1).unwrap())
        {
          if let (Backend::S3(s31), Backend::S3(s32)) = (location1.backend(), location2.backend()) {
            return (
              s31.bucket().to_string(),
              location1.prefix().to_string(),
              s32.bucket().to_string(),
              location2.prefix().to_string(),
            );
          }
        }

        panic!();
      },
    );
  }

  #[cfg(feature = "url")]
  #[test]
  fn location_url() {
    test_serialize_and_deserialize(
      r#"
      locations = [ "https://example.com/prefix1", "http://example.com/prefix2" ]
      "#,
      (
        "https://example.com/".to_string(),
        "prefix1".to_string(),
        "http://example.com/".to_string(),
        "prefix2".to_string(),
      ),
      |result: Config| {
        let result = result.locations.0;
        assert_eq!(result.len(), 2);
        if let (LocationEither::Simple(location1), LocationEither::Simple(location2)) =
          (result.first().unwrap(), result.get(1).unwrap())
        {
          if let (Backend::Url(url1), Backend::Url(url2)) =
            (location1.backend(), location2.backend())
          {
            return (
              url1.url().to_string(),
              location1.prefix().to_string(),
              url2.url().to_string(),
              location2.prefix().to_string(),
            );
          }
        }

        panic!();
      },
    );
  }

  fn assert_file_location(result: Config) -> (String, String) {
    let result = result.locations.0;
    assert_eq!(result.len(), 1);
    if let LocationEither::Simple(location1) = result.first().unwrap() {
      let file1 = location1.backend().as_file().unwrap();
      return (
        file1.local_path().to_string(),
        location1.prefix().to_string(),
      );
    }

    panic!();
  }
}
