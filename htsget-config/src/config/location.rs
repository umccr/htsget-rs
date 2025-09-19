//! Storage location configuration.
//!

use crate::config::advanced::regex_location::RegexLocation;
use crate::error::{Error::ParseError, Result};
use crate::storage::Backend;
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use crate::storage::file::default_authority;
use crate::types::Scheme;
use crate::{error, storage};
use cfg_if::cfg_if;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "url")]
use {crate::config::advanced::url::Url, http::Uri, http::uri::InvalidUri};

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
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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

  /// Get the storage backend as a mutable reference.
  pub fn backend_mut(&mut self) -> &mut Backend {
    match self {
      LocationEither::Simple(location) => location.backend_mut(),
      LocationEither::Regex(regex_location) => regex_location.backend_mut(),
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

/// Whether the location specifies a prefix or an exact match id.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub enum PrefixOrId {
  Prefix(String),
  Id(String),
}

impl PrefixOrId {
  /// Convert to a prefix if the variant is a prefix.
  pub fn as_prefix(&self) -> Option<&str> {
    match self {
      PrefixOrId::Prefix(prefix) => Some(prefix),
      PrefixOrId::Id(_) => None,
    }
  }

  /// Convert to an id if the variant is an id.
  pub fn as_id(&self) -> Option<&str> {
    match self {
      PrefixOrId::Prefix(_) => None,
      PrefixOrId::Id(id) => Some(id),
    }
  }
}

impl Default for PrefixOrId {
  fn default() -> Self {
    Self::Prefix(Default::default())
  }
}

/// Location config.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(
  default,
  try_from = "LocationWrapper",
  into = "LocationWrapper",
  deny_unknown_fields
)]
pub struct Location {
  backend: Backend,
  to_append: String,
  prefix_or_id: PrefixOrId,
}

impl Location {
  /// Create a new location.
  pub fn new(backend: Backend, to_append: String, prefix_or_id: PrefixOrId) -> Self {
    Self {
      backend,
      to_append,
      prefix_or_id,
    }
  }

  /// Get the storage backend.
  pub fn backend(&self) -> &Backend {
    &self.backend
  }

  /// Get the storage backend as a mutable reference
  pub fn backend_mut(&mut self) -> &mut Backend {
    &mut self.backend
  }

  /// Get the prefix or id.
  pub fn prefix_or_id(&self) -> &PrefixOrId {
    &self.prefix_or_id
  }

  /// Get the additional path to append to resolve the id.
  pub fn to_append(&self) -> &str {
    &self.to_append
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
  #[serde(flatten)]
  location: StringLocation,
  #[cfg(feature = "experimental")]
  #[serde(skip_serializing)]
  keys: Option<C4GHKeys>,
}

/// Deserialize the location from a string with a protocol.
#[derive(JsonSchema, Deserialize, Serialize, Debug, Clone, Default)]
#[serde(default, deny_unknown_fields)]
struct StringLocation {
  location: String,
  #[serde(flatten)]
  prefix_or_id: PrefixOrId,
}

/// Deserialize the location from a map with regular field and values.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default, deny_unknown_fields)]
struct MapLocation {
  location: Backend,
  append_to: String,
  prefix_or_id: PrefixOrId,
}

/// A wrapper around location deserialization that can deserialize either a string
/// or a map. This is required so that default values behave correctly when deserializing
/// the `Location`. For example, if a location string isn't specified, the `Deserialize`
/// implementation for `StringLocation` can't account for this as it gets passed default values
/// which contain map elements. This wrapper allows deserializing using regular semantics by
/// falling back to the regular `MapLocation` derived deserializer. The reason there needs to be a
/// `StringLocation` and `MapLocation` type is so that `Location` can be deserialized using the
/// `from` attribute without recursion.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
#[serde(untagged, deny_unknown_fields)]
enum LocationWrapper {
  SingleLocation(String),
  #[schemars(skip)]
  String(StringLocation),
  #[schemars(skip)]
  Map(Box<MapLocation>),
  #[schemars(skip)]
  Extended(ExtendedLocation),
}

impl From<Location> for LocationWrapper {
  fn from(location: Location) -> Self {
    LocationWrapper::Map(Box::from(MapLocation {
      location: location.backend,
      append_to: location.to_append,
      prefix_or_id: location.prefix_or_id,
    }))
  }
}

impl TryFrom<LocationWrapper> for Location {
  type Error = error::Error;

  fn try_from(location: LocationWrapper) -> Result<Self> {
    match location {
      LocationWrapper::SingleLocation(location) => {
        let backend: BackendWithAppend = location.try_into()?;
        Ok(Location::new(backend.0, backend.1, Default::default()))
      }
      LocationWrapper::String(wrapper) => {
        let backend: BackendWithAppend = wrapper.location.try_into()?;
        Ok(Location::new(backend.0, backend.1, wrapper.prefix_or_id))
      }
      LocationWrapper::Map(wrapper) => Ok(Location::new(
        wrapper.location,
        wrapper.append_to,
        wrapper.prefix_or_id,
      )),
      LocationWrapper::Extended(wrapper) => {
        cfg_if! {
          if #[cfg(feature = "experimental")] {
            let mut backend: BackendWithAppend = wrapper.location.location.try_into()?;
            backend.0.set_keys(wrapper.keys);
            Ok(Location::new(backend.0, backend.1, wrapper.location.prefix_or_id))
          } else {
            let backend: BackendWithAppend = wrapper.location.location.try_into()?;
            Ok(Location::new(backend.0, backend.1, wrapper.location.prefix_or_id))
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

/// Extracts the backend and the additional path that needs to be appended to resolve the id.
struct BackendWithAppend(Backend, String);

impl TryFrom<String> for BackendWithAppend {
  type Error = error::Error;

  fn try_from(s: String) -> Result<Self> {
    let split = |s: &str| {
      let (s1, s2) = if let Some(split) = s
        .split_once("/")
        .map(|(s1, s2)| (s1.to_string(), s2.to_string()))
      {
        split
      } else {
        (s.to_string(), "".to_string())
      };

      if s1.is_empty() {
        Err(ParseError("cannot have empty location".to_string()))
      } else {
        Ok((s1, s2))
      }
    };

    if let Some(s) = s.strip_prefix("file://") {
      let (path, to_append) = split(s)?;

      let mut file = storage::file::File::new(Scheme::Http, default_authority(), path.to_string());
      // Origin should be updated based on data server config.
      file.reset_origin = true;

      return Ok(BackendWithAppend(Backend::File(file), to_append));
    }

    #[cfg(feature = "aws")]
    if let Some(s) = s.strip_prefix("s3://") {
      let (bucket, to_append) = split(s)?;

      return Ok(BackendWithAppend(
        Backend::S3(storage::s3::S3::new(bucket.to_string(), None, false)),
        to_append,
      ));
    }

    #[cfg(feature = "url")]
    if let Some(s_stripped) = s
      .strip_prefix("http://")
      .or_else(|| s.strip_prefix("https://"))
    {
      let (mut uri, to_append) = split(s_stripped)?;

      if s.starts_with("http://") {
        uri = format!("http://{s_stripped}");
      }
      if s.starts_with("https://") {
        uri = format!("https://{s_stripped}");
      }

      let uri: Uri = uri
        .parse()
        .map_err(|err: InvalidUri| error::Error::ParseError(err.to_string()))?;
      let url = Url::new(uri.clone(), Some(uri), true, vec![], Default::default()).try_into()?;

      return Ok(BackendWithAppend(Backend::Url(url), to_append));
    }

    Err(ParseError(
      "expected file://, s3://, http:// or https:// scheme".to_string(),
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::Config;
  use crate::config::tests::test_serialize_and_deserialize;
  use std::result;

  #[test]
  fn location_single() {
    test_serialize_and_deserialize(
      r#"
      locations = "file://path/prefix1"
      "#,
      ("path".to_string(), "prefix1".to_string(), "".to_string()),
      |result: Config| assert_file_location(result),
    );
    test_serialize_and_deserialize(
      r#"
      locations = "file://path/prefix1/"
      "#,
      ("path".to_string(), "prefix1/".to_string(), "".to_string()),
      |result: Config| assert_file_location(result),
    );
  }

  #[test]
  fn location_no_prefix() {
    test_serialize_and_deserialize(
      r#"
      locations = "file://path"
      "#,
      ("path".to_string(), "".to_string(), "".to_string()),
      |result: Config| assert_file_location(result),
    );
    test_serialize_and_deserialize(
      r#"
      locations = "file://path/"
      "#,
      ("path".to_string(), "".to_string(), "".to_string()),
      |result: Config| assert_file_location(result),
    );
  }

  #[test]
  fn location_file() {
    test_serialize_and_deserialize(
      r#"
      locations = [ { location = "file://path", prefix = "prefix1" }, { location = "file://path", prefix = "prefix2" } ]
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
            location1.prefix_or_id().as_prefix().unwrap().to_string(),
            file2.local_path().to_string(),
            location2.prefix_or_id().as_prefix().unwrap().to_string(),
          );
        }

        panic!();
      },
    );
  }

  #[test]
  fn location_file_append_to() {
    let assert_fn = |result: Config| {
      let result = result.locations.0;
      assert_eq!(result.len(), 2);
      if let (LocationEither::Simple(location1), LocationEither::Simple(location2)) =
        (result.first().unwrap(), result.get(1).unwrap())
      {
        let file1 = location1.backend().as_file().unwrap();
        let file2 = location2.backend().as_file().unwrap();

        return (
          file1.local_path().to_string(),
          location1.to_append().to_string(),
          location1.prefix_or_id().as_prefix().unwrap().to_string(),
          file2.local_path().to_string(),
          location2.to_append().to_string(),
          location2.prefix_or_id().as_prefix().unwrap().to_string(),
        );
      }

      panic!();
    };

    test_serialize_and_deserialize(
      r#"
      locations = [ { location = "file://path/dir1", prefix = "prefix1" }, { location = "file://path/dir2", prefix = "prefix2" } ]
      "#,
      (
        "path".to_string(),
        "dir1".to_string(),
        "prefix1".to_string(),
        "path".to_string(),
        "dir2".to_string(),
        "prefix2".to_string(),
      ),
      assert_fn,
    );

    test_serialize_and_deserialize(
      r#"
      locations = [ { location = "file://path/dir1/", prefix = "prefix1" }, { location = "file://path/dir2/", prefix = "prefix2" } ]
      "#,
      (
        "path".to_string(),
        "dir1/".to_string(),
        "prefix1".to_string(),
        "path".to_string(),
        "dir2/".to_string(),
        "prefix2".to_string(),
      ),
      assert_fn,
    );

    test_serialize_and_deserialize(
      r#"
      locations = [ { location = "file://path/", prefix = "prefix1" }, { location = "file://path", prefix = "prefix2" } ]
      "#,
      (
        "path".to_string(),
        "".to_string(),
        "prefix1".to_string(),
        "path".to_string(),
        "".to_string(),
        "prefix2".to_string(),
      ),
      assert_fn,
    );
  }

  #[test]
  fn location_file_id() {
    test_serialize_and_deserialize(
      r#"
      locations = [ { location = "file://path", id = "id1" }, { location = "file://path", id = "id2" } ]
      "#,
      (
        "path".to_string(),
        "id1".to_string(),
        "path".to_string(),
        "id2".to_string(),
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
            location1.prefix_or_id().as_id().unwrap().to_string(),
            file2.local_path().to_string(),
            location2.prefix_or_id().as_id().unwrap().to_string(),
          );
        }

        panic!();
      },
    );
  }

  #[test]
  fn location_file_multiple_fail() {
    let config: result::Result<Config, _> = toml::from_str(
      r#"
      locations = [ { location = "file://path", id = "id1", prefix = "prefix1" }]
      "#,
    );
    assert!(config.is_err());
  }

  #[cfg(feature = "aws")]
  #[test]
  fn location_s3() {
    test_serialize_and_deserialize(
      r#"
      locations = [ { location = "s3://bucket", prefix = "prefix1" }, { location = "s3://bucket", prefix = "prefix2" } ]
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
              location1.prefix_or_id().as_prefix().unwrap().to_string(),
              s32.bucket().to_string(),
              location2.prefix_or_id().as_prefix().unwrap().to_string(),
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
      locations = [ { location = "https://example.com", prefix = "prefix1" }, { location = "http://example.com", prefix = "prefix2" } ]
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
              location1.prefix_or_id().as_prefix().unwrap().to_string(),
              url2.url().to_string(),
              location2.prefix_or_id().as_prefix().unwrap().to_string(),
            );
          }
        }

        panic!();
      },
    );
  }

  fn assert_file_location(result: Config) -> (String, String, String) {
    let result = result.locations.0;
    assert_eq!(result.len(), 1);
    if let LocationEither::Simple(location1) = result.first().unwrap() {
      let file1 = location1.backend().as_file().unwrap();
      return (
        file1.local_path().to_string(),
        location1.to_append().to_string(),
        location1.prefix_or_id().as_prefix().unwrap().to_string(),
      );
    }

    panic!();
  }
}
