pub mod cors;

use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use crate::regex_resolver::aws::S3Resolver;
use clap::Parser;
use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use http::header::{HeaderName, InvalidHeaderValue};
use http::{HeaderValue as HeaderValueInner, Method};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Error as DeError;
use serde::ser::Error as SeError;
use serde_with::with_prefix;
use tracing::info;
use tracing::instrument;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, fmt, Registry};
use crate::config::cors::{AllowType, CorsConfig, HeaderValue, TaggedAnyAllowType};

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

fn default_path() -> &'static str {
    "data"
}

pub(crate) fn default_serve_at() -> &'static str {
    "/data"
}

/// The command line arguments allowed for the htsget-rs executables.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = USAGE)]
struct Args {
    #[arg(short, long, env = "HTSGET_CONFIG")]
    config: PathBuf,
}

/// Configuration for the server. Each field will be read from environment variables.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Config {
    #[serde(flatten)]
    ticket_server: TicketServerConfig,
    data_servers: Vec<DataServerConfig>,
    resolvers: Vec<RegexResolver>,
}

with_prefix!(ticket_server_prefix "ticket_server_");

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct TicketServerConfig {
    ticket_server_addr: SocketAddr,
    #[serde(flatten, with = "ticket_server_prefix")]
    cors: CorsConfig,
    #[serde(flatten)]
    service_info: ServiceInfo,
}

impl TicketServerConfig {
    pub fn addr(&self) -> SocketAddr {
        self.ticket_server_addr
    }

    pub fn cors(&self) -> &CorsConfig {
        &self.cors
    }

    pub fn service_info(&self) -> &ServiceInfo {
        &self.service_info
    }

    pub fn allow_credentials(&self) -> bool {
        self.cors.allow_credentials()
    }

    pub fn allow_origins(&self) -> &AllowType<HeaderValue> {
        self.cors.allow_origins()
    }

    pub fn allow_headers(&self) -> &AllowType<HeaderName> {
        self.cors.allow_headers()
    }

    pub fn allow_methods(&self) -> &AllowType<Method> {
        self.cors.allow_methods()
    }

    pub fn max_age(&self) -> usize {
        self.cors.max_age()
    }

    pub fn expose_headers(&self) -> &AllowType<HeaderName, TaggedAnyAllowType> {
        self.cors.expose_headers()
    }

    pub fn id(&self) -> &Option<String> {
        self.service_info.id()
    }

    pub fn name(&self) -> &Option<String> {
        self.service_info.name()
    }

    pub fn version(&self) -> &Option<String> {
        self.service_info.version()
    }

    pub fn organization_name(&self) -> &Option<String> {
        self.service_info.organization_name()
    }

    pub fn organization_url(&self) -> &Option<String> {
        self.service_info.organization_url()
    }

    pub fn contact_url(&self) -> &Option<String> {
        self.service_info.contact_url()
    }

    pub fn documentation_url(&self) -> &Option<String> {
        self.service_info.documentation_url()
    }

    pub fn created_at(&self) -> &Option<String> {
        self.service_info.created_at()
    }

    pub fn updated_at(&self) -> &Option<String> {
        self.service_info.updated_at()
    }

    pub fn environment(&self) -> &Option<String> {
        self.service_info.environment()
    }
}

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct DataServerConfig {
    addr: SocketAddr,
    path: PathBuf,
    serve_at: PathBuf,
    key: Option<PathBuf>,
    cert: Option<PathBuf>,
    #[serde(flatten)]
    cors: CorsConfig,
}

impl DataServerConfig {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn serve_at(&self) -> &PathBuf {
        &self.serve_at
    }

    pub fn key(&self) -> &Option<PathBuf> {
        &self.key
    }

    pub fn cert(&self) -> &Option<PathBuf> {
        &self.cert
    }

    pub fn cors(&self) -> &CorsConfig {
        &self.cors
    }

    pub fn allow_credentials(&self) -> bool {
        self.cors.allow_credentials()
    }

    pub fn allow_origins(&self) -> &AllowType<HeaderValue> {
        self.cors.allow_origins()
    }

    pub fn allow_headers(&self) -> &AllowType<HeaderName> {
        self.cors.allow_headers()
    }

    pub fn allow_methods(&self) -> &AllowType<Method> {
        self.cors.allow_methods()
    }

    pub fn max_age(&self) -> usize {
        self.cors.max_age()
    }

    pub fn expose_headers(&self) -> &AllowType<HeaderName, TaggedAnyAllowType> {
        self.cors.expose_headers()
    }
}

impl Default for DataServerConfig {
    fn default() -> Self {
        Self {
            addr: default_localstorage_addr().parse().expect("expected valid address"),
            path: default_path().into(),
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
    pub fn id(&self) -> &Option<String> {
        &self.id
    }

    pub fn name(&self) -> &Option<String> {
        &self.name
    }

    pub fn version(&self) -> &Option<String> {
        &self.version
    }

    pub fn organization_name(&self) -> &Option<String> {
        &self.organization_name
    }

    pub fn organization_url(&self) -> &Option<String> {
        &self.organization_url
    }

    pub fn contact_url(&self) -> &Option<String> {
        &self.contact_url
    }

    pub fn documentation_url(&self) -> &Option<String> {
        &self.documentation_url
    }

    pub fn created_at(&self) -> &Option<String> {
        &self.created_at
    }

    pub fn updated_at(&self) -> &Option<String> {
        &self.updated_at
    }

    pub fn environment(&self) -> &Option<String> {
        &self.environment
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
            data_servers: vec![DataServerConfig::default()],
            resolvers: vec![RegexResolver::default(), RegexResolver::default()],
        }
    }
}

impl Config {
    /// Parse the command line arguments
    pub fn parse_args() -> PathBuf {
        Args::parse().config
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

    pub fn ticket_server(&self) -> &TicketServerConfig {
        &self.ticket_server
    }

    pub fn data_servers(&self) -> &Vec<DataServerConfig> {
        &self.data_servers
    }

    pub fn resolvers(&self) -> &Vec<RegexResolver> {
        &self.resolvers
    }
}

#[cfg(test)]
mod tests {
    use figment::Jail;
    use regex::Regex;
    use crate::config::cors::AllowType::Tagged;
    use crate::Format::Bam;
    use crate::regex_resolver::{Scheme, StorageType};
    use super::*;

    fn test_config<K, V, F>(contents: Option<&str>, env_variables: Vec<(K, V)>, test_fn: F)
    where
      K: AsRef<str>,
      V: Display,
      F: FnOnce(Config) {
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
      F: FnOnce(Config) {
        test_config(None, env_variables, test_fn);
    }

    fn test_config_from_file<F>(contents: &str, test_fn: F)
    where
      F: FnOnce(Config) {
        test_config(Some(contents), Vec::<(&str, &str)>::new(), test_fn);
    }

    #[test]
    fn config_ticket_server_addr_env() {
        test_config_from_env(vec![("HTSGET_TICKET_SERVER_ADDR", "127.0.0.1:8082")], |config| {
            assert_eq!(
              config.ticket_server().addr(),
              "127.0.0.1:8082".parse().unwrap()
            );
        });
    }

    #[test]
    fn config_ticket_server_cors_allow_origin_env() {
        test_config_from_env(vec![("HTSGET_TICKET_SERVER_ALLOW_CREDENTIALS", true)], |config| {
            assert!(
                config.ticket_server().allow_credentials()
            );
        });
    }

    #[test]
    fn config_service_info_id_env() {
        test_config_from_env(vec![("HTSGET_ID", "id")], |config| {
            assert_eq!(
                config.ticket_server().id(),
                &Some("id".to_string())
            );
        });
    }

    #[test]
    fn config_data_server_addr_env() {
        test_config_from_env( vec![("HTSGET_DATA_SERVERS", "[{addr=127.0.0.1:8082}]")], |config| {
            assert_eq!(
                config.data_servers().first().unwrap().addr(),
                "127.0.0.1:8082".parse().unwrap()
            );
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
        test_config_from_file(r#"ticket_server_allow_credentials = true"#, |config| {
            assert!(
                config.ticket_server().allow_credentials()
            );
        });
    }

    #[test]
    fn config_service_info_id_file() {
        test_config_from_file(r#"id = "id""#, |config| {
            assert_eq!(
                config.ticket_server().id(),
                &Some("id".to_string())
            );
        });
    }

    #[test]
    fn config_data_server_addr_file() {
        test_config_from_file(r#"
            [[data_servers]]
            addr = "127.0.0.1:8082"
        "#, |config| {
            assert_eq!(
                config.data_servers().first().unwrap().addr(),
                "127.0.0.1:8082".parse().unwrap()
            );
        });
    }

    #[test]
    fn config_resolvers_file() {
        test_config_from_file(r#"
            [[resolvers]]
            regex = "regex"
        "#, |config| {
            assert_eq!(
                config.resolvers().first().unwrap().regex().as_str(),
                "regex"
            );
        });
    }

    #[test]
    fn config_resolvers_guard_file() {
        test_config_from_file(r#"
            [[resolvers]]
            regex = "regex"

            [resolvers.guard]
            match_formats = ["BAM"]
        "#, |config| {
            assert_eq!(
                config.resolvers().first().unwrap().match_formats(),
                &vec![Bam]
            );
        });
    }

    #[test]
    fn config_storage_type_url_file() {
        test_config_from_file(r#"
            [[resolvers]]
            regex = "regex"

            [resolvers.storage_type]
            type = "Url"
            path = "path"
            scheme = "HTTPS"
        "#, |config| {
            assert!(matches!(
                config.resolvers().first().unwrap().storage_type(),
                StorageType::Url(resolver) if resolver.path() == "path" && resolver.scheme() == Scheme::Https
            ));
        });
    }

    #[cfg(feature = "s3-storage")]
    #[test]
    fn config_storage_type_s3_file() {
        test_config_from_file(r#"
            [[resolvers]]
            regex = "regex"

            [resolvers.storage_type]
            type = "S3"
            bucket = "bucket"
        "#, |config| {
            assert!(matches!(
                config.resolvers().first().unwrap().storage_type(),
                StorageType::S3(resolver) if resolver.bucket() == "bucket"
            ));
        });
    }
}
