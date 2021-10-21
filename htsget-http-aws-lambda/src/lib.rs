use std::sync::Arc;

use config::Config;

pub mod config;
pub mod handlers;

use htsget_search::{
  htsget::{from_storage::HtsGetFromStorage, HtsGet},
};

pub type AsyncHtsGetStorage = HtsGetFromStorage<S3Storage>;
pub struct AsyncAppState<H: HtsGet> {
  pub htsget: Arc<H>,
  pub config: Config,
}