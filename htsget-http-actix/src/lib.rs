use std::sync::Arc;

use config::Config;
use htsget_search::htsget::blocking::from_storage::HtsGetFromStorage;
use htsget_search::htsget::blocking::HtsGet;
use htsget_search::{
  htsget::{from_storage::HtsGetFromStorage as AsyncHtsGetFromStorage, HtsGet as AsyncHtsGet},
  storage::blocking::local::LocalStorage,
};

pub mod config;
pub mod handlers;

pub const USAGE: &str = r#"
This executable doesn't use command line arguments, but there are some environment variables that can be set to configure the HtsGet server:
* HTSGET_IP: The ip to use. Default: 127.0.0.1
* HTSGET_PORT: The port to use. Default: 8080
* HTSGET_PATH: The path to the directory where the server should be started. Default: Actual directory
* HTSGET_REGEX: The regular expression that should match an ID. Default: ".*"
* HTSGET_REPLACEMENT: The replacement expression. Default: "$0"
For more information about the regex options look in the documentation of the regex crate(https://docs.rs/regex/).
The next variables are used to configure the info for the service-info endpoints
* HTSGET_ID: The id of the service. Default: ""
* HTSGET_NAME: The name of the service. Default: "HtsGet service"
* HTSGET_VERSION: The version of the service. Default: ""
* HTSGET_ORGANIZATION_NAME: The name of the organization. Default: "Snake oil"
* HTSGET_ORGANIZATION_URL: The url of the organization. Default: "https://en.wikipedia.org/wiki/Snake_oil"
* HTSGET_CONTACT_URL: A url to provide contact to the users. Default: "",
* HTSGET_DOCUMENTATION_URL: A link to the documentation. Default: "https://github.com/umccr/htsget-rs/tree/main/htsget-http-actix",
* HTSGET_CREATED_AT: Date of the creation of the service. Default: "",
* HTSGET_UPDATED_AT: Date of the last update of the service. Default: "",
* HTSGET_ENVIRONMENT: The environment in which the service is running. Default: "Testing",
"#;

#[cfg(feature = "async")]
pub type AsyncHtsGetStorage = AsyncHtsGetFromStorage<LocalStorage>;
pub type HtsGetStorage = HtsGetFromStorage<LocalStorage>;

#[cfg(feature = "async")]
pub struct AsyncAppState<H: AsyncHtsGet> {
  pub htsget: Arc<H>,
  pub config: Config,
}

pub struct AppState<H: HtsGet> {
  pub htsget: H,
  pub config: Config,
}
