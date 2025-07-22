//! Structs to serialize and deserialize the htsget-rs config options.
//!

use std::fmt::Debug;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::advanced::FormattingStyle;
use crate::config::data_server::DataServerEnabled;
use crate::config::location::{Location, LocationEither, Locations};
use crate::config::parser::from_path;
use crate::config::service_info::ServiceInfo;
use crate::config::ticket_server::TicketServerConfig;
use crate::error::Error::{ArgParseError, ParseError, TracingError};
use crate::error::Result;
use crate::storage::file::File;
use crate::storage::Backend;
use clap::{Args as ClapArgs, Command, FromArgMatches, Parser};
use serde::{Deserialize, Serialize};
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
}

/// Simplified config.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
  ticket_server: TicketServerConfig,
  data_server: DataServerEnabled,
  service_info: ServiceInfo,
  locations: Locations,
  formatting_style: FormattingStyle,
}

impl Config {
  /// Create a config.
  pub fn new(
    formatting_style: FormattingStyle,
    ticket_server: TicketServerConfig,
    data_server: DataServerEnabled,
    service_info: ServiceInfo,
    locations: Locations,
  ) -> Self {
    Self {
      formatting_style,
      ticket_server,
      data_server,
      service_info,
      locations,
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

  /// Get the data server config.
  pub fn data_server(&self) -> &DataServerEnabled {
    &self.data_server
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
    } else {
      Some(args.config.unwrap_or_else(|| "".into()))
    }
  }

  /// Read a config struct from a TOML file.
  pub fn from_path(path: &Path) -> io::Result<Self> {
    let config: Self = from_path(path)?;
    Ok(config.resolvers_from_data_server_config()?)
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
  pub fn resolvers_from_data_server_config(mut self) -> Result<Self> {
    self
      .locations
      .as_mut_slice()
      .iter_mut()
      .map(|location| {
        if let LocationEither::Simple(simple) = location {
          // Fall through only if the backend is File and default
          let file_location = if let Ok(location) = simple.backend().as_file() {
            location
          } else {
            return Ok(());
          };

          if let DataServerEnabled::Some(ref data_server) = self.data_server {
            let prefix = simple.prefix().to_string();

            // Don't update the local path as that comes in from the config.
            let file: File = data_server.try_into()?;
            let file = file.set_local_path(file_location.local_path().to_string());

            *location =
              LocationEither::Simple(Box::new(Location::new(Backend::File(file), prefix)));
          }
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
    }
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::fmt::Display;

  use super::*;
  use crate::config::parser::from_str;
  use crate::tls::tests::with_test_certificates;
  use crate::types::Scheme;
  use figment::Jail;
  use http::uri::Authority;
  #[cfg(feature = "url")]
  use http::Uri;
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
          .resolvers_from_data_server_config()
          .map_err(|err| err.to_string())?,
      );
      test_fn(
        from_str::<Config>(contents.unwrap_or(""))
          .map_err(|err| err.to_string())?
          .resolvers_from_data_server_config()
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
          assert!(config
            .data_server()
            .clone()
            .as_data_server_config()
            .unwrap()
            .tls()
            .is_none());
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
          assert!(config
            .data_server()
            .clone()
            .as_data_server_config()
            .unwrap()
            .tls()
            .is_some());
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
          assert!(config
            .data_server()
            .clone()
            .as_data_server_config()
            .unwrap()
            .tls()
            .is_some());
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
            Backend::File(file) if file.local_path() == "path" && file.scheme() == Scheme::Http && file.authority() == &Authority::from_static("127.0.0.1:8081")));
      },
    );
  }

  #[test]
  fn simple_locations_env() {
    test_config_from_env(
      vec![
        ("HTSGET_DATA_SERVER_ADDR", "127.0.0.1:8080"),
        ("HTSGET_LOCATIONS", "[file://data/bam, file://data/cram]"),
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
    data_server.local_path = "path"
    
    locations = "file://data"
    "#,
      |config| {
        assert_eq!(config.locations().len(), 1);
        let config = config.locations.into_inner();
        let location = config[0].as_simple().unwrap();
        assert_eq!(location.prefix(), "");
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
        assert_eq!(location.prefix(), "");
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
        assert_eq!(location.prefix(), "");
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
    locations = ["file://data/bam", "file://data/cram"]
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
    data_server.local_path = "root"
    locations = ["file://dir_one/bam", "file://dir_two/cram", "s3://bucket/vcf"]
    "#,
      |config| {
        assert_eq!(config.locations().len(), 3);
        let config = config.locations.into_inner();

        let location = config[0].as_simple().unwrap();
        assert_eq!(location.prefix(), "bam");
        assert_file_location(location, "dir_one");

        let location = config[1].as_simple().unwrap();
        assert_eq!(location.prefix(), "cram");
        assert_file_location(location, "dir_two");

        let location = config[2].as_simple().unwrap();
        assert_eq!(location.prefix(), "vcf");
        assert!(matches!(location.backend(),
            Backend::S3(s3) if s3.bucket() == "bucket"));
      },
    );
  }

  #[test]
  fn no_data_server() {
    test_config_from_file(
      r#"
      data_server = "None"
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
    assert_eq!(location.prefix(), "bam");
    assert_file_location(location, "data");

    let location = config[1].as_simple().unwrap();
    assert_eq!(location.prefix(), "cram");
    assert_file_location(location, "data");
  }

  fn assert_file_location(location: &Location, local_path: &str) {
    assert!(matches!(location.backend(),
            Backend::File(file) if file.local_path() == local_path && file.scheme() == Scheme::Http && file.authority() == &Authority::from_static("127.0.0.1:8080")));
  }
}
