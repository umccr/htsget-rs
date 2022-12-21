pub mod cors;

use std::fmt::Debug;
use std::io;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use crate::config::cors::{AllowType, CorsConfig, HeaderValue, TaggedAllowTypes};
use clap::Parser;
use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use http::header::HeaderName;
use http::Method;
use serde::{Deserialize, Serialize};
use serde_with::with_prefix;
use tracing::info;
use tracing::instrument;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{fmt, EnvFilter, Registry};

use crate::regex_resolver::RegexResolver;

/// Represents a usage string for htsget-rs.
pub const USAGE: &str = r#"
Available environment variables:
* HTSGET_PATH: The path to the directory where the server should be started. Default: "data". Unused if HTSGET_STORAGE_TYPE is "AwsS3Storage".
* HTSGET_REGEX: The regular expression that should match an ID. Default: ".*".
For more information about the regex options look in the documentation of the regex crate(https://docs.rs/regex/).
* HTSGET_SUBSTITUTION_STRING: The replacement expression. Default: "$0".
* HTSGET_STORAGE_TYPE: Either "LocalStorage" or "AwsS3Storage", representing which storage type to use. Default: "LocalStorage".

The following options are used for the ticket server.
* HTSGET_TICKET_SERVER_ADDR: The socket address for the server which creates response tickets. Default: "127.0.0.1:8080".
* HTSGET_TICKET_SERVER_ALLOW_CREDENTIALS: Boolean flag, indicating whether authenticated requests are allowed by including the `Access-Control-Allow-Credentials` header. Default: "false".
* HTSGET_TICKET_SERVER_ALLOW_ORIGIN: Which origin os allowed in the `ORIGIN` header. Default: "http://localhost:8080".

The following options are used for the data server.
* HTSGET_DATA_SERVER_ADDR: The socket address to use for the server which responds to tickets. Default: "127.0.0.1:8081". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_DATA_SERVER_KEY: The path to the PEM formatted X.509 private key used by the data server. Default: "None". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_DATA_SERVER_CERT: The path to the PEM formatted X.509 certificate used by the data server. Default: "None". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_DATA_SERVER_ALLOW_CREDENTIALS: Boolean flag, indicating whether authenticated requests are allowed by including the `Access-Control-Allow-Credentials` header. Default: "false"
* HTSGET_DATA_SERVER_ALLOW_ORIGIN: Which origin os allowed in the `ORIGIN` header. Default: "http://localhost:8081"

The following options are used to configure AWS S3 storage.
* HTSGET_S3_BUCKET: The name of the AWS S3 bucket. Default: "". Unused if HTSGET_STORAGE_TYPE is not "AwsS3Storage".

The next variables are used to configure the info for the service-info endpoints.
* HTSGET_ID: The id of the service. Default: "None".
* HTSGET_NAME: The name of the service. Default: "None".
* HTSGET_VERSION: The version of the service. Default: "None".
* HTSGET_ORGANIZATION_NAME: The name of the organization. Default: "None".
* HTSGET_ORGANIZATION_URL: The url of the organization. Default: "None".
* HTSGET_CONTACT_URL: A url to provide contact to the users. Default: "None".
* HTSGET_DOCUMENTATION_URL: A link to the documentation. Default: "None".
* HTSGET_CREATED_AT: Date of the creation of the service. Default: "None".
* HTSGET_UPDATED_AT: Date of the last update of the service. Default: "None".
* HTSGET_ENVIRONMENT: The environment in which the service is running. Default: "None".
"#;

const ENVIRONMENT_VARIABLE_PREFIX: &str = "HTSGET_";

pub(crate) fn default_localstorage_addr() -> &'static str {
  "127.0.0.1:8081"
}

fn default_addr() -> &'static str {
  "127.0.0.1:8080"
}

fn default_server_origin() -> &'static str {
  "http://localhost:8080"
}

pub(crate) fn default_path() -> &'static str {
  "data"
}

pub(crate) fn default_serve_at() -> &'static str {
  "/data"
}

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

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Config {
  #[serde(flatten)]
  ticket_server: TicketServerConfig,
  data_server: DataServerConfigOption,
  resolvers: Vec<RegexResolver>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum DataServerConfigNone {
  #[serde(alias = "none", alias = "NONE", alias = "")]
  None,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
enum DataServerConfigOption {
  None(DataServerConfigNone),
  Some(DataServerConfig),
}

with_prefix!(ticket_server_cors_prefix "ticket_server_cors_");

/// Configuration for the htsget ticket server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct TicketServerConfig {
  ticket_server_addr: SocketAddr,
  #[serde(flatten, with = "ticket_server_cors_prefix")]
  cors: CorsConfig,
  #[serde(flatten)]
  service_info: ServiceInfo,
}

impl TicketServerConfig {
  /// Create a new ticket server config.
  pub fn new(ticket_server_addr: SocketAddr, cors: CorsConfig, service_info: ServiceInfo) -> Self {
    Self {
      ticket_server_addr,
      cors,
      service_info,
    }
  }

  /// Get the addr.
  pub fn addr(&self) -> SocketAddr {
    self.ticket_server_addr
  }

  /// Get cors config.
  pub fn cors(&self) -> &CorsConfig {
    &self.cors
  }

  /// Get service info.
  pub fn service_info(&self) -> &ServiceInfo {
    &self.service_info
  }

  /// Get allow credentials.
  pub fn allow_credentials(&self) -> bool {
    self.cors.allow_credentials()
  }

  /// Get allow origins.
  pub fn allow_origins(&self) -> &AllowType<HeaderValue, TaggedAllowTypes> {
    self.cors.allow_origins()
  }

  /// Get allow headers.
  pub fn allow_headers(&self) -> &AllowType<HeaderName> {
    self.cors.allow_headers()
  }

  /// Get allow methods.
  pub fn allow_methods(&self) -> &AllowType<Method> {
    self.cors.allow_methods()
  }

  /// Get max age.
  pub fn max_age(&self) -> usize {
    self.cors.max_age()
  }

  /// Get expose headers.
  pub fn expose_headers(&self) -> &AllowType<HeaderName> {
    self.cors.expose_headers()
  }

  /// Get id.
  pub fn id(&self) -> Option<&str> {
    self.service_info.id()
  }

  /// Get name.
  pub fn name(&self) -> Option<&str> {
    self.service_info.name()
  }

  /// Get version.
  pub fn version(&self) -> Option<&str> {
    self.service_info.version()
  }

  /// Get organization name.
  pub fn organization_name(&self) -> Option<&str> {
    self.service_info.organization_name()
  }

  /// Get the organization url.
  pub fn organization_url(&self) -> Option<&str> {
    self.service_info.organization_url()
  }

  /// Get the contact url.
  pub fn contact_url(&self) -> Option<&str> {
    self.service_info.contact_url()
  }

  /// Get the documentation url.
  pub fn documentation_url(&self) -> Option<&str> {
    self.service_info.documentation_url()
  }

  /// Get created at.
  pub fn created_at(&self) -> Option<&str> {
    self.service_info.created_at()
  }

  /// Get updated at.
  pub fn updated_at(&self) -> Option<&str> {
    self.service_info.updated_at()
  }

  /// Get the environment.
  pub fn environment(&self) -> Option<&str> {
    self.service_info.environment()
  }
}

with_prefix!(cors_prefix "cors_");

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct DataServerConfig {
  addr: SocketAddr,
  local_path: PathBuf,
  serve_at: PathBuf,
  key: Option<PathBuf>,
  cert: Option<PathBuf>,
  #[serde(flatten, with = "cors_prefix")]
  cors: CorsConfig,
}

impl DataServerConfig {
  /// Create a new data server config.
  pub fn new(
    addr: SocketAddr,
    local_path: PathBuf,
    serve_at: PathBuf,
    key: Option<PathBuf>,
    cert: Option<PathBuf>,
    cors: CorsConfig,
  ) -> Self {
    Self {
      addr,
      local_path,
      serve_at,
      key,
      cert,
      cors,
    }
  }

  /// Get the address.
  pub fn addr(&self) -> SocketAddr {
    self.addr
  }

  /// Get the local path.
  pub fn local_path(&self) -> &Path {
    &self.local_path
  }

  /// Get the serve at path.
  pub fn serve_at(&self) -> &Path {
    &self.serve_at
  }

  /// Get the key.
  pub fn key(&self) -> Option<&Path> {
    self.key.as_deref()
  }

  /// Get the cert.
  pub fn cert(&self) -> Option<&Path> {
    self.cert.as_deref()
  }

  /// Get cors config.
  pub fn cors(&self) -> &CorsConfig {
    &self.cors
  }

  /// Get allow credentials.
  pub fn allow_credentials(&self) -> bool {
    self.cors.allow_credentials()
  }

  /// Get allow origins.
  pub fn allow_origins(&self) -> &AllowType<HeaderValue, TaggedAllowTypes> {
    self.cors.allow_origins()
  }

  /// Get allow headers.
  pub fn allow_headers(&self) -> &AllowType<HeaderName> {
    self.cors.allow_headers()
  }

  /// Get allow methods.
  pub fn allow_methods(&self) -> &AllowType<Method> {
    self.cors.allow_methods()
  }

  /// Get the max age.
  pub fn max_age(&self) -> usize {
    self.cors.max_age()
  }

  /// Get the expose headers.
  pub fn expose_headers(&self) -> &AllowType<HeaderName> {
    self.cors.expose_headers()
  }
}

impl Default for DataServerConfig {
  fn default() -> Self {
    Self {
      addr: default_localstorage_addr()
        .parse()
        .expect("expected valid address"),
      local_path: default_path().into(),
      serve_at: default_serve_at().into(),
      key: None,
      cert: None,
      cors: CorsConfig::default(),
    }
  }
}

/// Configuration of the service info.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct ServiceInfo {
  id: Option<String>,
  name: Option<String>,
  version: Option<String>,
  organization_name: Option<String>,
  organization_url: Option<String>,
  contact_url: Option<String>,
  documentation_url: Option<String>,
  created_at: Option<String>,
  updated_at: Option<String>,
  environment: Option<String>,
}

impl ServiceInfo {
  /// Get the id.
  pub fn id(&self) -> Option<&str> {
    self.id.as_deref()
  }

  /// Get the name.
  pub fn name(&self) -> Option<&str> {
    self.name.as_deref()
  }

  /// Get the version.
  pub fn version(&self) -> Option<&str> {
    self.version.as_deref()
  }

  /// Get the organization name.
  pub fn organization_name(&self) -> Option<&str> {
    self.organization_name.as_deref()
  }

  /// Get the organization url.
  pub fn organization_url(&self) -> Option<&str> {
    self.organization_url.as_deref()
  }

  /// Get the contact url.
  pub fn contact_url(&self) -> Option<&str> {
    self.contact_url.as_deref()
  }

  /// Get the documentation url.
  pub fn documentation_url(&self) -> Option<&str> {
    self.documentation_url.as_deref()
  }

  /// Get created at.
  pub fn created_at(&self) -> Option<&str> {
    self.created_at.as_deref()
  }

  /// Get updated at.
  pub fn updated_at(&self) -> Option<&str> {
    self.updated_at.as_deref()
  }

  /// Get environment.
  pub fn environment(&self) -> Option<&str> {
    self.environment.as_deref()
  }
}

impl Default for TicketServerConfig {
  fn default() -> Self {
    Self {
      ticket_server_addr: default_addr().parse().expect("expected valid address"),
      cors: CorsConfig::default(),
      service_info: ServiceInfo::default(),
    }
  }
}

impl Default for Config {
  fn default() -> Self {
    Self {
      ticket_server: TicketServerConfig::default(),
      data_server: DataServerConfigOption::Some(DataServerConfig::default()),
      resolvers: vec![RegexResolver::default()],
    }
  }
}

impl Config {
  /// Create a new config.
  pub fn new(
    ticket_server: TicketServerConfig,
    data_server: Option<DataServerConfig>,
    resolvers: Vec<RegexResolver>,
  ) -> Self {
    Self {
      ticket_server,
      data_server: match data_server {
        None => DataServerConfigOption::None(DataServerConfigNone::None),
        Some(value) => DataServerConfigOption::Some(value),
      },
      resolvers,
    }
  }

  /// Parse the command line arguments
  pub fn parse_args() -> Option<PathBuf> {
    let args = Args::parse();

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

  /// Read the environment variables into a Config struct.
  #[instrument]
  pub fn from_env(config: PathBuf) -> io::Result<Self> {
    let config = Figment::from(Serialized::defaults(Config::default()))
      .merge(Toml::file(config))
      .merge(Env::prefixed(ENVIRONMENT_VARIABLE_PREFIX))
      .extract()
      .map_err(|err| {
        io::Error::new(ErrorKind::Other, format!("failed to parse config: {}", err))
      })?;

    info!(config = ?config, "config created from environment variables");
    Ok(config)
  }

  /// Setup tracing, using a global subscriber.
  pub fn setup_tracing() -> io::Result<()> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = fmt::Layer::default();

    let subscriber = Registry::default().with(env_filter).with(fmt_layer);

    tracing::subscriber::set_global_default(subscriber).map_err(|err| {
      io::Error::new(
        ErrorKind::Other,
        format!("failed to install `tracing` subscriber: {}", err),
      )
    })?;

    Ok(())
  }

  /// Get the ticket server.
  pub fn ticket_server(&self) -> &TicketServerConfig {
    &self.ticket_server
  }

  /// Get the data server.
  pub fn data_server(&self) -> Option<&DataServerConfig> {
    match self.data_server {
      DataServerConfigOption::None(_) => None,
      DataServerConfigOption::Some(ref config) => Some(config),
    }
  }

  /// Get the resolvers.
  pub fn resolvers(&self) -> &[RegexResolver] {
    &self.resolvers
  }

  /// Get owned resolvers.
  pub fn owned_resolvers(self) -> Vec<RegexResolver> {
    self.resolvers
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::regex_resolver::{Scheme, StorageType};
  use crate::Format::Bam;
  use figment::Jail;
  use std::fmt::Display;

  fn test_config<K, V, F>(contents: Option<&str>, env_variables: Vec<(K, V)>, test_fn: F)
  where
    K: AsRef<str>,
    V: Display,
    F: FnOnce(Config),
  {
    Jail::expect_with(|jail| {
      if let Some(contents) = contents {
        jail.create_file("test.toml", contents)?;
      }

      for (key, value) in env_variables {
        jail.set_env(key, value);
      }

      test_fn(Config::from_env("test.toml".into()).map_err(|err| err.to_string())?);

      Ok(())
    });
  }

  fn test_config_from_env<K, V, F>(env_variables: Vec<(K, V)>, test_fn: F)
  where
    K: AsRef<str>,
    V: Display,
    F: FnOnce(Config),
  {
    test_config(None, env_variables, test_fn);
  }

  fn test_config_from_file<F>(contents: &str, test_fn: F)
  where
    F: FnOnce(Config),
  {
    test_config(Some(contents), Vec::<(&str, &str)>::new(), test_fn);
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
        assert!(config.ticket_server().allow_credentials());
      },
    );
  }

  #[test]
  fn config_service_info_id_env() {
    test_config_from_env(vec![("HTSGET_ID", "id")], |config| {
      assert_eq!(config.ticket_server().id(), Some("id"));
    });
  }

  #[test]
  fn config_data_server_addr_env() {
    test_config_from_env(
      vec![("HTSGET_DATA_SERVER", "{addr=127.0.0.1:8082}")],
      |config| {
        assert_eq!(
          config.data_server().unwrap().addr(),
          "127.0.0.1:8082".parse().unwrap()
        );
      },
    );
  }

  #[test]
  fn config_no_data_server_env() {
    test_config_from_env(vec![("HTSGET_DATA_SERVER", "")], |config| {
      assert!(matches!(config.data_server(), None));
    });
  }

  #[test]
  fn config_resolvers_env() {
    test_config_from_env(vec![("HTSGET_RESOLVERS", "[{regex=regex}]")], |config| {
      assert_eq!(
        config.resolvers().first().unwrap().regex().as_str(),
        "regex"
      );
    });
  }

  #[test]
  fn config_ticket_server_addr_file() {
    test_config_from_file(r#"ticket_server_addr = "127.0.0.1:8082""#, |config| {
      assert_eq!(
        config.ticket_server().addr(),
        "127.0.0.1:8082".parse().unwrap()
      );
    });
  }

  #[test]
  fn config_ticket_server_cors_allow_origin_file() {
    test_config_from_file(r#"ticket_server_cors_allow_credentials = true"#, |config| {
      assert!(config.ticket_server().allow_credentials());
    });
  }

  #[test]
  fn config_service_info_id_file() {
    test_config_from_file(r#"id = "id""#, |config| {
      assert_eq!(config.ticket_server().id(), Some("id"));
    });
  }

  #[test]
  fn config_data_server_addr_file() {
    test_config_from_file(
      r#"
            [data_server]
            addr = "127.0.0.1:8082"
        "#,
      |config| {
        assert_eq!(
          config.data_server().unwrap().addr(),
          "127.0.0.1:8082".parse().unwrap()
        );
      },
    );
  }

  #[test]
  fn config_no_data_server_file() {
    test_config_from_file(r#"data_server = """#, |config| {
      assert!(matches!(config.data_server(), None));
    });
  }

  #[test]
  fn config_resolvers_file() {
    test_config_from_file(
      r#"
            [[resolvers]]
            regex = "regex"
        "#,
      |config| {
        assert_eq!(
          config.resolvers().first().unwrap().regex().as_str(),
          "regex"
        );
      },
    );
  }

  #[test]
  fn config_resolvers_guard_file() {
    test_config_from_file(
      r#"
            [[resolvers]]
            regex = "regex"

            [resolvers.guard]
            allow_formats = ["BAM"]
        "#,
      |config| {
        assert_eq!(
          config.resolvers().first().unwrap().allow_formats(),
          &vec![Bam]
        );
      },
    );
  }

  #[test]
  fn config_storage_type_local_file() {
    test_config_from_file(
      r#"
            [[resolvers]]
            regex = "regex"

            [resolvers.storage_type]
            type = "Local"
            local_path = "path"
            scheme = "HTTPS"
            path_prefix = "path"
        "#,
      |config| {
        assert!(matches!(
            config.resolvers().first().unwrap().storage_type(),
            StorageType::Local(resolver) if resolver.local_path() == "path" && resolver.scheme() == Scheme::Https && resolver.path_prefix() == "path"
        ));
      },
    );
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn config_storage_type_s3_file() {
    test_config_from_file(
      r#"
            [[resolvers]]
            regex = "regex"

            [resolvers.storage_type]
            type = "S3"
            bucket = "bucket"
        "#,
      |config| {
        assert!(matches!(
            config.resolvers().first().unwrap().storage_type(),
            StorageType::S3(resolver) if resolver.bucket() == "bucket"
        ));
      },
    );
  }
}
