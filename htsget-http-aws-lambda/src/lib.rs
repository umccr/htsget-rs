#[cfg(feature = "async")]
use std::sync::Arc;

use config::Config;

#[cfg(not(feature = "async"))]
use htsget_search::htsget::blocking::HtsGet;
#[cfg(feature = "async")]
use htsget_search::{
  htsget::{from_storage::HtsGetFromStorage, HtsGet},
  storage::blocking::local::LocalStorage,
};

pub mod config;
pub mod handlers;

#[cfg(feature = "async")]
pub struct AsyncAppState<H: HtsGet> {
  pub htsget: Arc<H>,
  pub config: Config,
}

#[cfg(not(feature = "async"))]
pub struct AppState<H: HtsGet> {
  pub htsget: H,
  pub config: Config,
}
