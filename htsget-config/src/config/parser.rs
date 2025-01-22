//! Parse config for a file and environment variables.
//!

use crate::config::Config;
use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use serde::Deserialize;
use std::fmt::Debug;
use std::io;
use std::io::ErrorKind;
use std::path::Path;
use tracing::info;

const ENVIRONMENT_VARIABLE_PREFIX: &str = "HTSGET_";

/// A struct to represent a string or a path, used for parsing and deserializing config.
#[derive(Debug)]
pub enum Parser<'a> {
  String(&'a str),
  Path(&'a Path),
}

impl Parser<'_> {
  /// Deserialize a string or path into a config value using Figment.
  pub fn deserialize_config_into<T>(&self) -> io::Result<T>
  where
    for<'de> T: Deserialize<'de> + Debug,
  {
    let config = Figment::from(Serialized::defaults(Config::default()))
      .merge(match self {
        Parser::String(string) => Toml::string(string),
        Parser::Path(path) => Toml::file(path),
      })
      .merge(
        Env::prefixed(ENVIRONMENT_VARIABLE_PREFIX)
          .filter(|k| k != "config")
          .map(|k| {
            // This has to list all possible nested values to resolve issues with ambiguity when
            // deserializing. E.g. see https://github.com/SergioBenitez/Figment/issues/12
            k.as_str()
              .to_lowercase()
              .replace("ticket_server_", "ticket_server.")
              .replace("data_server_", "data_server.")
              .replace("cors_", "cors.")
              .replace("tls_", "tls.")
              .into()
          }),
      )
      .extract()
      .map_err(|err| io::Error::new(ErrorKind::Other, format!("failed to parse config: {err}")))?;

    info!(config = ?config, "config created");

    Ok(config)
  }
}

/// Read a deserializable config struct from a TOML file.
pub fn from_path<T>(path: &Path) -> io::Result<T>
where
  for<'a> T: Deserialize<'a> + Debug,
{
  Parser::Path(path).deserialize_config_into()
}

/// Read a deserializable config struct from a str.
pub fn from_str<T>(str: &str) -> io::Result<T>
where
  for<'a> T: Deserialize<'a> + Debug,
{
  Parser::String(str).deserialize_config_into()
}
