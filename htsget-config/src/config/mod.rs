//! Structs to serialize and deserialize the htsget-rs config options.
//!

use crate::config::advanced::FormattingStyle;
use crate::config::advanced::auth::{AuthConfig, AuthorizationRestrictions};
use crate::config::data_server::{DataServerConfig, DataServerEnabled};
use crate::config::location::{LocationEither, Locations};
use crate::config::parser::from_path;
use crate::config::service_info::ServiceInfo;
use crate::config::ticket_server::TicketServerConfig;
use crate::error::Error::{ArgParseError, ParseError, TracingError};
use crate::error::Result;
use crate::http::KeyPairScheme;
use crate::storage::Backend;
use clap::{Args as ClapArgs, Command, FromArgMatches, Parser};
use http::header::AUTHORIZATION;
use http::uri::Authority;
use schemars::schema_for;
use serde::de::Error;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;
use std::fmt::{Debug, Display};
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::subscriber::set_global_default;
use tracing_subscriber::fmt::{format, layer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

pub mod advanced;
pub mod data_server;
pub mod location;
pub mod parser;
pub mod service_info;
pub mod ticket_server;

/// The usage string for htsget-rs.
pub const USAGE: &str = "To configure htsget-rs use a config file or environment variables. \
See the documentation of the htsget-config crate for more information.";

/// The command line arguments allowed for the htsget-rs executables.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = USAGE)]
struct Args {
  #[arg(
    short,
    long,
    env = "HTSGET_CONFIG",
    help = "Set the location of the config file"
  )]
  config: Option<PathBuf>,
  #[arg(short, long, exclusive = true, help = "Print a default config file")]
  print_default_config: bool,
  #[arg(
    short = 's',
    long,
    exclusive = true,
    help = "Print the response JSON schema used in the htsget auth process"
  )]
  print_response_schema: bool,
}

/// Simplified config.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
  ticket_server: TicketServerConfig,
  data_server: DataServerEnabled,
  service_info: ServiceInfo,
  #[serde(alias = "location")]
  locations: Locations,
  formatting_style: FormattingStyle,
  #[serde(skip_serializing)]
  auth: Option<AuthConfig>,
}

impl Config {
  /// Create a config.
  pub fn new(
    formatting_style: FormattingStyle,
    ticket_server: TicketServerConfig,
    data_server: DataServerEnabled,
    service_info: ServiceInfo,
    locations: Locations,
    auth: Option<AuthConfig>,
  ) -> Self {
    Self {
      formatting_style,
      ticket_server,
      data_server,
      service_info,
      locations,
      auth,
    }
  }

  /// Get the ticket server config.
  pub fn formatting_style(&self) -> FormattingStyle {
    self.formatting_style
  }

  /// Get the ticket server config.
  pub fn ticket_server(&self) -> &TicketServerConfig {
    &self.ticket_server
  }

  /// Get the mutable ticket server config.
  pub fn ticket_server_mut(&mut self) -> &mut TicketServerConfig {
    &mut self.ticket_server
  }

  /// Get the data server config.
  pub fn data_server(&self) -> &DataServerEnabled {
    &self.data_server
  }

  /// Get the mutable data server config.
  pub fn data_server_mut(&mut self) -> Option<&mut DataServerConfig> {
    match &mut self.data_server {
      DataServerEnabled::None(_) => None,
      DataServerEnabled::Some(data_server) => Some(data_server),
    }
  }

  /// Get the service info config.
  pub fn service_info(&self) -> &ServiceInfo {
    &self.service_info
  }

  /// Get a mutable instance of the service info config.
  pub fn service_info_mut(&mut self) -> &mut ServiceInfo {
    &mut self.service_info
  }

  /// Get the location.
  pub fn locations(&self) -> &[LocationEither] {
    self.locations.as_slice()
  }

  pub fn into_locations(self) -> Locations {
    self.locations
  }

  /// Parse the command line arguments. Returns the config path, or prints the default config.
  /// Augment the `Command` args from the `clap` parser. Returns an error if the
  pub fn parse_args_with_command(augment_args: Command) -> Result<Option<PathBuf>> {
    let args = Args::from_arg_matches(&Args::augment_args(augment_args).get_matches())
      .map_err(|err| ArgParseError(err.to_string()))?;

    if args.config.as_ref().is_some_and(|path| !path.exists()) {
      return Err(ParseError("config file not found".to_string()));
    }

    Ok(Self::parse_with_args(args))
  }

  /// Parse the command line arguments. Returns the config path, or prints the default config.
  pub fn parse_args() -> Option<PathBuf> {
    Self::parse_with_args(Args::parse())
  }

  fn parse_with_args(args: Args) -> Option<PathBuf> {
    if args.print_default_config {
      println!(
        "{}",
        toml::ser::to_string_pretty(&Config::default()).unwrap()
      );
      None
    } else if args.print_response_schema {
      println!(
        "{}",
        serde_json::to_string_pretty(&schema_for!(AuthorizationRestrictions)).unwrap()
      );
      None
    } else {
      Some(args.config.unwrap_or_else(|| "".into()))
    }
  }

  /// Read a config struct from a TOML file.
  pub fn from_path(path: &Path) -> io::Result<Self> {
    let mut config: Self = from_path(path)?;

    // Propagate global config to individual ticket and data servers.
    if let DataServerEnabled::Some(ref mut data_server_config) = config.data_server {
      if data_server_config.auth().is_none() {
        data_server_config.set_auth(config.auth.clone());
      }
    }
    if config.ticket_server().auth().is_none() {
      config.ticket_server.set_auth(config.auth.clone());
    }

    Ok(config.validate_file_locations()?)
  }

  /// Setup tracing, using a global subscriber.
  pub fn setup_tracing(&self) -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = Registry::default().with(env_filter);

    match self.formatting_style() {
      FormattingStyle::Full => set_global_default(subscriber.with(layer())),
      FormattingStyle::Compact => {
        set_global_default(subscriber.with(layer().event_format(format().compact())))
      }
      FormattingStyle::Pretty => {
        set_global_default(subscriber.with(layer().event_format(format().pretty())))
      }
      FormattingStyle::Json => {
        set_global_default(subscriber.with(layer().event_format(format().json())))
      }
    }
    .map_err(|err| TracingError(err.to_string()))?;

    Ok(())
  }

  /// Set the local resolvers from the data server config.
  pub fn validate_file_locations(mut self) -> Result<Self> {
    if !self
      .locations()
      .iter()
      .any(|location| location.backend().as_file().is_ok())
    {
      return Ok(self);
    }

    let DataServerEnabled::Some(ref mut config) = self.data_server else {
      return Err(ParseError(
        "must enable data server if using file locations".to_string(),
      ));
    };

    let mut possible_paths: HashSet<_> =
      HashSet::from_iter(self.locations.as_slice().iter().map(|location| {
        location
          .backend()
          .as_file()
          .ok()
          .map(|file| file.local_path())
      }));
    possible_paths.remove(&None);

    if possible_paths.len() > 1 {
      return Err(ParseError(
        "cannot have multiple file paths for file storage".to_string(),
      ));
    }
    let local_path = possible_paths
      .into_iter()
      .next()
      .flatten()
      .ok_or_else(|| ParseError("failed to find local path from locations".to_string()))?
      .to_string();

    if config
      .local_path()
      .is_some_and(|path| path.to_string_lossy() != local_path)
    {
      return Err(ParseError(
        "the data server local path and file storage directories must be the same".to_string(),
      ));
    }

    config.set_local_path(Some(PathBuf::from(local_path)));

    let scheme = config.tls().get_scheme();
    let authority =
      Authority::from_str(&config.addr().to_string()).map_err(|err| ParseError(err.to_string()))?;
    let ticket_origin = config.ticket_origin();

    self
      .locations
      .as_mut_slice()
      .iter_mut()
      .map(|location| {
        // Configure the scheme and authority for file locations that haven't been
        // explicitly set.
        match location.backend_mut() {
          Backend::File(file) => {
            if file.is_defaulted {
              file.set_scheme(scheme);
              file.set_authority(authority.clone());
              file.set_ticket_origin(ticket_origin.clone())
            }
          }
          #[cfg(feature = "aws")]
          Backend::S3(_) => {}
          #[cfg(feature = "url")]
          Backend::Url(_) => {}
        }

        // Ensure authorization header gets forwarded if the data server has authorization set.
        if self
          .data_server
          .as_data_server_config()
          .is_ok_and(|config| config.auth().is_some())
        {
          location
            .backend_mut()
            .add_ticket_header(AUTHORIZATION.to_string());
        }

        Ok(())
      })
      .collect::<Result<Vec<()>>>()?;

    Ok(self)
  }
}

impl Default for Config {
  fn default() -> Self {
    Self {
      formatting_style: FormattingStyle::Full,
      ticket_server: Default::default(),
      data_server: DataServerEnabled::Some(Default::default()),
      service_info: Default::default(),
      locations: Default::default(),
      auth: Default::default(),
    }
  }
}

pub(crate) fn serialize_array_display<S, T>(
  names: &[T],
  serializer: S,
) -> std::result::Result<S::Ok, S::Error>
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

pub(crate) fn deserialize_vec_from_str<'de, D, T>(
  deserializer: D,
) -> std::result::Result<Vec<T>, D::Error>
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

#[cfg(test)]
pub(crate) mod tests {
  use std::fmt::Display;

  use super::*;
  use crate::config::advanced::auth::authorization::UrlOrStatic;
  use crate::config::advanced::auth::jwt::AuthMode;
  use crate::config::location::Location;
  use crate::config::parser::from_str;
  use crate::http::tests::with_test_certificates;
  use crate::storage::Backend;
  use crate::types::Scheme;
  use figment::Jail;
  use http::Uri;
  use http::uri::Authority;
  use serde::de::DeserializeOwned;
  use serde_json::json;

  fn test_config<K, V, F>(contents: Option<&str>, env_variables: Vec<(K, V)>, test_fn: F)
  where
    K: AsRef<str>,
    V: Display,
    F: Fn(Config),
  {
    Jail::expect_with(|jail| {
      let file = "test.toml";

      if let Some(contents) = contents {
        jail.create_file(file, contents)?;
      }

      for (key, value) in env_variables {
        jail.set_env(key, value);
      }

      let path = Path::new(file);
      test_fn(Config::from_path(path).map_err(|err| err.to_string())?);

      test_fn(
        from_path::<Config>(path)
          .map_err(|err| err.to_string())?
          .validate_file_locations()
          .map_err(|err| err.to_string())?,
      );
      test_fn(
        from_str::<Config>(contents.unwrap_or(""))
          .map_err(|err| err.to_string())?
          .validate_file_locations()
          .map_err(|err| err.to_string())?,
      );

      Ok(())
    });
  }

  pub(crate) fn test_config_from_env<K, V, F>(env_variables: Vec<(K, V)>, test_fn: F)
  where
    K: AsRef<str>,
    V: Display,
    F: Fn(Config),
  {
    test_config(None, env_variables, test_fn);
  }

  pub(crate) fn test_config_from_file<F>(contents: &str, test_fn: F)
  where
    F: Fn(Config),
  {
    test_config(Some(contents), Vec::<(&str, &str)>::new(), test_fn);
  }

  pub(crate) fn test_serialize_and_deserialize<T, D, F>(input: &str, expected: T, get_result: F)
  where
    T: Debug + PartialEq,
    F: Fn(D) -> T,
    D: DeserializeOwned + Serialize + Clone,
  {
    let config: D = toml::from_str(input).unwrap();
    assert_eq!(expected, get_result(config.clone()));

    let serialized = toml::to_string(&config).unwrap();
    let deserialized = toml::from_str(&serialized).unwrap();
    assert_eq!(expected, get_result(deserialized));
  }

  #[test]
  fn config_ticket_server_addr_env() {
    test_config_from_env(
      vec![("HTSGET_TICKET_SERVER_ADDR", "127.0.0.1:8082")],
      |config| {
        assert_eq!(
          config.ticket_server().addr(),
          "127.0.0.1:8082".parse().unwrap()
        );
      },
    );
  }

  #[test]
  fn config_ticket_server_cors_allow_origin_env() {
    test_config_from_env(
      vec![("HTSGET_TICKET_SERVER_CORS_ALLOW_CREDENTIALS", true)],
      |config| {
        assert!(config.ticket_server().cors().allow_credentials());
      },
    );
  }

  #[test]
  fn config_service_info_id_env() {
    test_config_from_env(vec![("HTSGET_SERVICE_INFO", "{ id=id }")], |config| {
      assert_eq!(config.service_info().as_ref().get("id"), Some(&json!("id")));
    });
  }

  #[test]
  fn config_data_server_addr_env() {
    test_config_from_env(
      vec![("HTSGET_DATA_SERVER_ADDR", "127.0.0.1:8082")],
      |config| {
        assert_eq!(
          config
            .data_server()
            .clone()
            .as_data_server_config()
            .unwrap()
            .addr(),
          "127.0.0.1:8082".parse().unwrap()
        );
      },
    );
  }

  #[test]
  fn config_ticket_server_addr_file() {
    test_config_from_file(r#"ticket_server.addr = "127.0.0.1:8082""#, |config| {
      assert_eq!(
        config.ticket_server().addr(),
        "127.0.0.1:8082".parse().unwrap()
      );
    });
  }

  #[test]
  fn config_ticket_server_cors_allow_origin_file() {
    test_config_from_file(r#"ticket_server.cors.allow_credentials = true"#, |config| {
      assert!(config.ticket_server().cors().allow_credentials());
    });
  }

  #[test]
  fn config_service_info_id_file() {
    test_config_from_file(r#"service_info.id = "id""#, |config| {
      assert_eq!(config.service_info().as_ref().get("id"), Some(&json!("id")));
    });
  }

  #[test]
  fn config_data_server_addr_file() {
    test_config_from_file(r#"data_server.addr = "127.0.0.1:8082""#, |config| {
      assert_eq!(
        config
          .data_server()
          .clone()
          .as_data_server_config()
          .unwrap()
          .addr(),
        "127.0.0.1:8082".parse().unwrap()
      );
    });
  }

  #[test]
  #[should_panic]
  fn config_data_server_tls_no_cert() {
    with_test_certificates(|path, _, _| {
      let key_path = path.join("key.pem");

      test_config_from_file(
        &format!(
          r#"
        data_server.tls.key = "{}"
        "#,
          key_path.to_string_lossy().escape_default()
        ),
        |config| {
          assert!(
            config
              .data_server()
              .clone()
              .as_data_server_config()
              .unwrap()
              .tls()
              .is_none()
          );
        },
      );
    });
  }

  #[test]
  fn config_data_server_tls() {
    with_test_certificates(|path, _, _| {
      let key_path = path.join("key.pem");
      let cert_path = path.join("cert.pem");

      test_config_from_file(
        &format!(
          r#"
          data_server.tls.key = "{}"
          data_server.tls.cert = "{}"
          "#,
          key_path.to_string_lossy().escape_default(),
          cert_path.to_string_lossy().escape_default()
        ),
        |config| {
          assert!(
            config
              .data_server()
              .clone()
              .as_data_server_config()
              .unwrap()
              .tls()
              .is_some()
          );
        },
      );
    });
  }

  #[test]
  fn config_data_server_tls_env() {
    with_test_certificates(|path, _, _| {
      let key_path = path.join("key.pem");
      let cert_path = path.join("cert.pem");

      test_config_from_env(
        vec![
          ("HTSGET_DATA_SERVER_TLS_KEY", key_path.to_string_lossy()),
          ("HTSGET_DATA_SERVER_TLS_CERT", cert_path.to_string_lossy()),
        ],
        |config| {
          assert!(
            config
              .data_server()
              .clone()
              .as_data_server_config()
              .unwrap()
              .tls()
              .is_some()
          );
        },
      );
    });
  }

  #[test]
  #[should_panic]
  fn config_ticket_server_tls_no_cert() {
    with_test_certificates(|path, _, _| {
      let key_path = path.join("key.pem");

      test_config_from_file(
        &format!(
          r#"
        ticket_server.tls.key = "{}"
        "#,
          key_path.to_string_lossy().escape_default()
        ),
        |config| {
          assert!(config.ticket_server().tls().is_none());
        },
      );
    });
  }

  #[test]
  fn config_ticket_server_tls() {
    with_test_certificates(|path, _, _| {
      let key_path = path.join("key.pem");
      let cert_path = path.join("cert.pem");

      test_config_from_file(
        &format!(
          r#"
        ticket_server.tls.key = "{}"
        ticket_server.tls.cert = "{}"
        "#,
          key_path.to_string_lossy().escape_default(),
          cert_path.to_string_lossy().escape_default()
        ),
        |config| {
          assert!(config.ticket_server().tls().is_some());
        },
      );
    });
  }

  #[test]
  fn config_ticket_server_tls_env() {
    with_test_certificates(|path, _, _| {
      let key_path = path.join("key.pem");
      let cert_path = path.join("cert.pem");

      test_config_from_env(
        vec![
          ("HTSGET_TICKET_SERVER_TLS_KEY", key_path.to_string_lossy()),
          ("HTSGET_TICKET_SERVER_TLS_CERT", cert_path.to_string_lossy()),
        ],
        |config| {
          assert!(config.ticket_server().tls().is_some());
        },
      );
    });
  }

  #[test]
  fn locations_from_data_server_config() {
    test_config_from_file(
      r#"
    data_server.addr = "127.0.0.1:8080"
    data_server.local_path = "path"

    [[locations]]
    regex = "123"
    backend.kind = "File"
    backend.local_path = "path"
    "#,
      |config| {
        assert_eq!(config.locations().len(), 1);
        let config = config.locations.into_inner();
        let regex = config[0].as_regex().unwrap();
        assert!(matches!(regex.backend(),
            Backend::File(file) if file.local_path() == "path" && file.scheme() == Scheme::Http && file.authority() == &Authority::from_static("127.0.0.1:8080")));
      },
    );
  }

  #[test]
  fn simple_locations_env() {
    test_config_from_env(
      vec![
        ("HTSGET_DATA_SERVER_ADDR", "127.0.0.1:8080"),
        (
          "HTSGET_LOCATIONS",
          "[ { location=file://data, prefix=bam }, { location=file://data, prefix=cram }]",
        ),
      ],
      |config| {
        assert_multiple(config);
      },
    );
  }

  #[test]
  fn simple_locations() {
    test_config_from_file(
      r#"
    data_server.addr = "127.0.0.1:8080"
    data_server.local_path = "data"
    
    locations = "file://data"
    "#,
      |config| {
        assert_eq!(config.locations().len(), 1);
        let config = config.locations.into_inner();
        let location = config[0].as_simple().unwrap();
        assert_eq!(location.prefix_or_id().unwrap().as_prefix().unwrap(), "");
        assert_file_location(location, "data");
      },
    );
  }

  #[cfg(feature = "aws")]
  #[test]
  fn simple_locations_s3() {
    test_config_from_file(
      r#"
    locations = "s3://bucket"
    "#,
      |config| {
        assert_eq!(config.locations().len(), 1);
        let config = config.locations.into_inner();
        let location = config[0].as_simple().unwrap();
        assert_eq!(location.prefix_or_id().unwrap().as_prefix().unwrap(), "");
        assert!(matches!(location.backend(),
            Backend::S3(s3) if s3.bucket() == "bucket"));
      },
    );
  }

  #[cfg(feature = "url")]
  #[test]
  fn simple_locations_url() {
    test_config_from_file(
      r#"
    locations = "https://example.com"
    "#,
      |config| {
        assert_eq!(config.locations().len(), 1);
        let config = config.locations.into_inner();
        let location = config[0].as_simple().unwrap();
        assert_eq!(location.prefix_or_id().unwrap().as_prefix().unwrap(), "");
        assert!(matches!(location.backend(),
            Backend::Url(url) if url.url() == &"https://example.com".parse::<Uri>().unwrap()));
      },
    );
  }

  #[test]
  fn simple_locations_multiple() {
    test_config_from_file(
      r#"
    data_server.addr = "127.0.0.1:8080"
    locations = [ { location = "file://data", prefix = "bam" }, { location = "file://data", prefix = "cram" }]
    "#,
      |config| {
        assert_multiple(config);
      },
    );
  }

  #[cfg(feature = "aws")]
  #[test]
  fn simple_locations_multiple_mixed() {
    test_config_from_file(
      r#"
    data_server.addr = "127.0.0.1:8080"
    data_server.local_path = "data"
    locations = [ { location = "file://data", prefix = "bam" }, { location = "file://data", prefix = "cram" }, { location = "s3://bucket", prefix = "vcf" } ]
    "#,
      |config| {
        assert_eq!(config.locations().len(), 3);
        let config = config.locations.into_inner();

        let location = config[0].as_simple().unwrap();
        assert_eq!(location.prefix_or_id().unwrap().as_prefix().unwrap(), "bam");
        assert_file_location(location, "data");

        let location = config[1].as_simple().unwrap();
        assert_eq!(
          location.prefix_or_id().unwrap().as_prefix().unwrap(),
          "cram"
        );
        assert_file_location(location, "data");

        let location = config[2].as_simple().unwrap();
        assert_eq!(location.prefix_or_id().unwrap().as_prefix().unwrap(), "vcf");
        assert!(matches!(location.backend(),
            Backend::S3(s3) if s3.bucket() == "bucket"));
      },
    );
  }

  #[test]
  fn config_server_auth() {
    test_config_from_file(
      r#"
      ticket_server.auth.jwks_url = "https://www.example.com/"
      ticket_server.auth.validate_issuer = ["iss1"]
      ticket_server.auth.authorization_url = "https://www.example.com"
      data_server.auth.jwks_url = "https://www.example.com/"
      data_server.auth.validate_audience = ["aud1"]
      data_server.auth.authorization_url = "https://www.example.com"
      "#,
      |config| {
        let auth = config.ticket_server().auth().unwrap();
        assert_eq!(
          auth.auth_mode().unwrap(),
          &AuthMode::Jwks("https://www.example.com/".parse().unwrap())
        );
        assert_eq!(
          auth.validate_issuer(),
          Some(vec!["iss1".to_string()].as_slice())
        );
        assert_eq!(
          auth.authorization_url().unwrap(),
          &UrlOrStatic::Url("https://www.example.com".parse::<Uri>().unwrap())
        );
        let auth = config
          .data_server()
          .as_data_server_config()
          .unwrap()
          .auth()
          .unwrap();
        assert_eq!(
          auth.auth_mode().unwrap(),
          &AuthMode::Jwks("https://www.example.com/".parse().unwrap())
        );
        assert_eq!(
          auth.validate_audience(),
          Some(vec!["aud1".to_string()].as_slice())
        );
        assert_eq!(
          auth.authorization_url().unwrap(),
          &UrlOrStatic::Url("https://www.example.com".parse::<Uri>().unwrap())
        );
      },
    );
  }

  #[test]
  fn config_server_auth_global() {
    test_config_from_file(
      r#"
      auth.jwks_url = "https://www.example.com/"
      auth.validate_audience = ["aud1"]
      auth.authorization_url = "https://www.example.com"
      "#,
      |config| {
        let auth = config.auth.unwrap();
        assert_eq!(
          auth.auth_mode().unwrap(),
          &AuthMode::Jwks("https://www.example.com/".parse().unwrap())
        );
        assert_eq!(
          auth.validate_audience(),
          Some(vec!["aud1".to_string()].as_slice())
        );
        assert_eq!(
          auth.authorization_url().unwrap(),
          &UrlOrStatic::Url("https://www.example.com".parse::<Uri>().unwrap())
        );
      },
    );
  }

  #[cfg(feature = "aws")]
  #[test]
  fn no_data_server() {
    test_config_from_file(
      r#"
      data_server = "None"
      locations = "s3://bucket"
    "#,
      |config| {
        assert!(config.data_server().as_data_server_config().is_err());
      },
    );
  }

  fn assert_multiple(config: Config) {
    assert_eq!(config.locations().len(), 2);
    let config = config.locations.into_inner();

    println!("{config:#?}");

    let location = config[0].as_simple().unwrap();
    assert_eq!(location.prefix_or_id().unwrap().as_prefix().unwrap(), "bam");
    assert_file_location(location, "data");

    let location = config[1].as_simple().unwrap();
    assert_eq!(
      location.prefix_or_id().unwrap().as_prefix().unwrap(),
      "cram"
    );
    assert_file_location(location, "data");
  }

  fn assert_file_location(location: &Location, local_path: &str) {
    assert!(matches!(location.backend(),
            Backend::File(file) if file.local_path() == local_path && file.scheme() == Scheme::Http && file.authority() == &Authority::from_static("127.0.0.1:8080")));
  }
}
