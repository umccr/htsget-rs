use std::fmt::Debug;
use std::io;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

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

use crate::config::cors::{AllowType, CorsConfig, HeaderValue, TaggedAllowTypes};
use crate::resolver::Resolver;
use crate::types::Scheme;
use crate::types::Scheme::{Http, Https};

pub mod cors;

/// Represents a usage string for htsget-rs.
pub const USAGE: &str =
  "htsget-rs can be configured using a config file or environment variables. \
See the documentation of the htsget-config crate for more information.";

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

with_prefix!(data_server_prefix "data_server_");

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Config {
  #[serde(flatten)]
  ticket_server: TicketServerConfig,
  #[serde(flatten, with = "data_server_prefix")]
  data_server: DataServerConfig,
  resolvers: Vec<Resolver>,
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

/// A trait to determine which scheme a key pair option has.
pub trait KeyPairScheme {
  /// Get the scheme.
  fn get_scheme(&self) -> Scheme;
}

/// A certificate and key pair used for TLS.
/// This is the path to the PEM formatted X.509 certificate and private key.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CertificateKeyPair {
  cert: PathBuf,
  key: PathBuf,
}

impl CertificateKeyPair {
  /// Create a new certificate key pair.
  pub fn new(cert: PathBuf, key: PathBuf) -> Self {
    Self { cert, key }
  }

  /// Get the cert.
  pub fn cert(&self) -> &Path {
    &self.cert
  }

  /// Get the key.
  pub fn key(&self) -> &Path {
    &self.key
  }
}

impl KeyPairScheme for Option<&CertificateKeyPair> {
  fn get_scheme(&self) -> Scheme {
    match self {
      None => Http,
      Some(_) => Https,
    }
  }
}

with_prefix!(cors_prefix "cors_");

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct DataServerConfig {
  enabled: bool,
  addr: SocketAddr,
  local_path: PathBuf,
  serve_at: String,
  #[serde(flatten)]
  tls: Option<CertificateKeyPair>,
  #[serde(flatten, with = "cors_prefix")]
  cors: CorsConfig,
}

impl DataServerConfig {
  /// Create a new data server config.
  pub fn new(
    enabled: bool,
    addr: SocketAddr,
    local_path: PathBuf,
    serve_at: String,
    tls: Option<CertificateKeyPair>,
    cors: CorsConfig,
  ) -> Self {
    Self {
      enabled,
      addr,
      local_path,
      serve_at,
      tls,
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
  pub fn serve_at(&self) -> &str {
    &self.serve_at
  }

  /// Get the TLS config
  pub fn tls(&self) -> Option<&CertificateKeyPair> {
    self.tls.as_ref()
  }

  /// Get the TLS config
  pub fn into_tls(self) -> Option<CertificateKeyPair> {
    self.tls
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

  /// Is the data server disabled
  pub fn enabled(&self) -> bool {
    self.enabled
  }
}

impl Default for DataServerConfig {
  fn default() -> Self {
    Self {
      enabled: true,
      addr: default_localstorage_addr()
        .parse()
        .expect("expected valid address"),
      local_path: default_path().into(),
      serve_at: default_serve_at().into(),
      tls: None,
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
      data_server: DataServerConfig::default(),
      resolvers: vec![Resolver::default()],
    }
  }
}

impl Config {
  /// Create a new config.
  pub fn new(
    ticket_server: TicketServerConfig,
    data_server: DataServerConfig,
    resolvers: Vec<Resolver>,
  ) -> Self {
    Self {
      ticket_server,
      data_server,
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

  /// Read a config struct from a TOML file.
  #[instrument]
  pub fn from_path(path: &Path) -> io::Result<Self> {
    let config: Config = Figment::from(Serialized::defaults(Config::default()))
      .merge(Toml::file(path))
      .merge(Env::prefixed(ENVIRONMENT_VARIABLE_PREFIX))
      .extract()
      .map_err(|err| io::Error::new(ErrorKind::Other, format!("failed to parse config: {err}")))?;

    info!(config = ?config, "config created from environment variables");

    Ok(config.resolvers_from_data_server_config())
  }

  /// Setup tracing, using a global subscriber.
  pub fn setup_tracing() -> io::Result<()> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = fmt::Layer::default();

    let subscriber = Registry::default().with(env_filter).with(fmt_layer);

    tracing::subscriber::set_global_default(subscriber).map_err(|err| {
      io::Error::new(
        ErrorKind::Other,
        format!("failed to install `tracing` subscriber: {err}"),
      )
    })?;

    Ok(())
  }

  /// Get the ticket server.
  pub fn ticket_server(&self) -> &TicketServerConfig {
    &self.ticket_server
  }

  /// Get the data server.
  pub fn data_server(&self) -> &DataServerConfig {
    &self.data_server
  }

  /// Get the owned data server.
  pub fn into_data_server(self) -> DataServerConfig {
    self.data_server
  }

  /// Get the resolvers.
  pub fn resolvers(&self) -> &[Resolver] {
    &self.resolvers
  }

  /// Get owned resolvers.
  pub fn owned_resolvers(self) -> Vec<Resolver> {
    self.resolvers
  }

  /// Set the local resolvers from the data server config.
  pub fn resolvers_from_data_server_config(self) -> Self {
    let Config {
      ticket_server,
      data_server,
      mut resolvers,
    } = self;
    resolvers
      .iter_mut()
      .for_each(|resolver| resolver.resolvers_from_data_server_config(&data_server));

    Self::new(ticket_server, data_server, resolvers)
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::fmt::Display;

  use figment::Jail;
  use http::uri::Authority;

  use crate::storage::Storage;

  use super::*;

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

      test_fn(Config::from_path(Path::new("test.toml")).map_err(|err| err.to_string())?);

      Ok(())
    });
  }

  pub(crate) fn test_config_from_env<K, V, F>(env_variables: Vec<(K, V)>, test_fn: F)
  where
    K: AsRef<str>,
    V: Display,
    F: FnOnce(Config),
  {
    test_config(None, env_variables, test_fn);
  }

  pub(crate) fn test_config_from_file<F>(contents: &str, test_fn: F)
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
      vec![("HTSGET_DATA_SERVER_ADDR", "127.0.0.1:8082")],
      |config| {
        assert_eq!(
          config.data_server().addr(),
          "127.0.0.1:8082".parse().unwrap()
        );
      },
    );
  }

  #[test]
  fn config_no_data_server_env() {
    test_config_from_env(vec![("HTSGET_DATA_SERVER_ENABLED", "true")], |config| {
      assert!(config.data_server().enabled());
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
    test_config_from_file(r#"data_server_addr = "127.0.0.1:8082""#, |config| {
      assert_eq!(
        config.data_server().addr(),
        "127.0.0.1:8082".parse().unwrap()
      );
    });
  }

  #[test]
  fn config_no_data_server_file() {
    test_config_from_file(r#"data_server_enabled = true"#, |config| {
      assert!(config.data_server().enabled());
    });
  }

  #[test]
  fn resolvers_from_data_server_config() {
    test_config_from_file(
      r#"
    data_server_addr = "127.0.0.1:8080"
    data_server_local_path = "path"
    data_server_serve_at = "/path"

    [[resolvers]]
    storage = "Local"
    "#,
      |config| {
        assert_eq!(config.resolvers.len(), 1);

        assert!(matches!(config.resolvers.first().unwrap().storage(),
      Storage::Local { local_storage } if local_storage.local_path() == "path" && local_storage.scheme() == Http && local_storage.authority() == &Authority::from_static("127.0.0.1:8080") && local_storage.path_prefix() == "/path"));
      },
    );
  }
}
