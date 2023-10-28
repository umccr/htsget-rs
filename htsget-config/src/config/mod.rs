use std::fmt::Debug;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use clap::{Args as ClapArgs, Command, FromArgMatches, Parser};
use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use http::header::HeaderName;
use http::Method;
use serde::{Deserialize, Serialize};
use serde_with::with_prefix;
use tracing::subscriber::set_global_default;
use tracing_subscriber::fmt::{format, layer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

use crate::config::cors::{AllowType, CorsConfig, HeaderValue, TaggedAllowTypes};
use crate::config::FormattingStyle::{Compact, Full, Json, Pretty};
use crate::error::Error::{ArgParseError, IoError, TracingError};
use crate::error::Result;
use crate::resolver::Resolver;
use crate::tls::TlsServerConfig;

pub mod cors;

/// Represents a usage string for htsget-rs.
pub const USAGE: &str = "To configure htsget-rs use a config file or environment variables. \
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

/// Determines which tracing formatting style to use.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Default)]
pub enum FormattingStyle {
  #[default]
  Full,
  Compact,
  Pretty,
  Json,
}

with_prefix!(ticket_server_prefix "ticket_server_");
with_prefix!(data_server_prefix "data_server_");
with_prefix!(cors_prefix "cors_");

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Config {
  formatting_style: FormattingStyle,
  #[serde(flatten, with = "ticket_server_prefix")]
  ticket_server: TicketServerConfig,
  #[serde(flatten, with = "data_server_prefix")]
  data_server: DataServerConfig,
  #[serde(flatten)]
  service_info: ServiceInfo,
  resolvers: Vec<Resolver>,
}

/// Configuration for the htsget ticket server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct TicketServerConfig {
  addr: SocketAddr,
  #[serde(skip_serializing)]
  tls: Option<TlsServerConfig>,
  #[serde(flatten, with = "cors_prefix")]
  cors: CorsConfig,
}

impl TicketServerConfig {
  /// Create a new ticket server config.
  pub fn new(addr: SocketAddr, tls: Option<TlsServerConfig>, cors: CorsConfig) -> Self {
    Self { addr, tls, cors }
  }

  /// Get the addr.
  pub fn addr(&self) -> SocketAddr {
    self.addr
  }

  /// Get the TLS config.
  pub fn tls(&self) -> Option<&TlsServerConfig> {
    self.tls.as_ref()
  }

  /// Get the TLS config.
  pub fn into_tls(self) -> Option<TlsServerConfig> {
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

  /// Get max age.
  pub fn max_age(&self) -> usize {
    self.cors.max_age()
  }

  /// Get expose headers.
  pub fn expose_headers(&self) -> &AllowType<HeaderName> {
    self.cors.expose_headers()
  }
}

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct DataServerConfig {
  enabled: bool,
  addr: SocketAddr,
  local_path: PathBuf,
  serve_at: String,
  #[serde(skip_serializing)]
  tls: Option<TlsServerConfig>,
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
    tls: Option<TlsServerConfig>,
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

  /// Get the TLS config.
  pub fn tls(&self) -> Option<&TlsServerConfig> {
    self.tls.as_ref()
  }

  /// Get the TLS config.
  pub fn into_tls(self) -> Option<TlsServerConfig> {
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
      addr: default_addr().parse().expect("expected valid address"),
      tls: None,
      cors: CorsConfig::default(),
    }
  }
}

impl Default for Config {
  fn default() -> Self {
    Self {
      formatting_style: Full,
      ticket_server: TicketServerConfig::default(),
      data_server: DataServerConfig::default(),
      service_info: ServiceInfo::default(),
      resolvers: vec![Resolver::default()],
    }
  }
}

impl Config {
  /// Create a new config.
  pub fn new(
    formatting: FormattingStyle,
    ticket_server: TicketServerConfig,
    data_server: DataServerConfig,
    service_info: ServiceInfo,
    resolvers: Vec<Resolver>,
  ) -> Self {
    Self {
      formatting_style: formatting,
      ticket_server,
      data_server,
      service_info,
      resolvers,
    }
  }

  /// Parse the command line arguments. Returns the config path, or prints the default config.
  /// Augment the `Command` args from the `clap` parser. Returns an error if the
  pub fn parse_args_with_command(augment_args: Command) -> Result<Option<PathBuf>> {
    Ok(Self::parse_with_args(
      Args::from_arg_matches(&Args::augment_args(augment_args).get_matches())
        .map_err(|err| ArgParseError(err.to_string()))?,
    ))
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
  pub fn from_path(path: &Path) -> Result<Self> {
    let config: Config = Figment::from(Serialized::defaults(Config::default()))
      .merge(Toml::file(path))
      .merge(Env::prefixed(ENVIRONMENT_VARIABLE_PREFIX).map(|k| match k {
        k if k.as_str().to_lowercase().contains("tls_") => {
          k.as_str().to_lowercase().replace("tls_", "tls.").into()
        }
        k => k.into(),
      }))
      .extract()
      .map_err(|err| IoError(err.to_string()))?;

    config.resolvers_from_data_server_config()
  }

  /// Setup tracing, using a global subscriber.
  pub fn setup_tracing(&self) -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = Registry::default().with(env_filter);

    match self.formatting_style() {
      Full => set_global_default(subscriber.with(layer())),
      Compact => set_global_default(subscriber.with(layer().event_format(format().compact()))),
      Pretty => set_global_default(subscriber.with(layer().event_format(format().pretty()))),
      Json => set_global_default(subscriber.with(layer().event_format(format().json()))),
    }
    .map_err(|err| TracingError(err.to_string()))?;

    Ok(())
  }

  /// Get the formatting style.
  pub fn formatting_style(&self) -> FormattingStyle {
    self.formatting_style
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

  /// Get service info.
  pub fn service_info(&self) -> &ServiceInfo {
    &self.service_info
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
  pub fn resolvers_from_data_server_config(self) -> Result<Self> {
    let Config {
      formatting_style: formatting,
      ticket_server,
      data_server,
      service_info,
      mut resolvers,
    } = self;

    resolvers
      .iter_mut()
      .for_each(|resolver| resolver.resolvers_from_data_server_config(&data_server));

    Ok(Self::new(
      formatting,
      ticket_server,
      data_server,
      service_info,
      resolvers,
    ))
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::fmt::Display;

  use figment::Jail;
  use http::uri::Authority;

  use crate::storage::Storage;
  use crate::tls::tests::with_test_certificates;
  use crate::types::Scheme::Http;

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
      assert_eq!(config.service_info().id(), Some("id"));
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
      assert_eq!(config.service_info().id(), Some("id"));
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
  #[should_panic]
  fn config_data_server_tls_no_cert() {
    with_test_certificates(|path, _, _| {
      let key_path = path.join("key.pem");

      test_config_from_file(
        &format!(
          r#"
        data_server_tls.key = "{}"
        "#,
          key_path.to_string_lossy().escape_default()
        ),
        |config| {
          assert!(config.data_server().tls().is_none());
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
          data_server_tls.key = "{}"
          data_server_tls.cert = "{}"
          "#,
          key_path.to_string_lossy().escape_default(),
          cert_path.to_string_lossy().escape_default()
        ),
        |config| {
          println!("{:?}", config.data_server().tls());
          assert!(config.data_server().tls().is_some());
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
          assert!(config.data_server().tls().is_some());
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
        ticket_server_tls.key = "{}"
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
        ticket_server_tls.key = "{}"
        ticket_server_tls.cert = "{}"
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
