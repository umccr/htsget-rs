use std::sync::Arc;

use config::Config;
use htsget_search::htsget::HtsGet;

pub mod config;
pub mod handlers;

pub struct AsyncAppState<H: HtsGet> {
  pub htsget: Arc<H>,
  pub config: Config,
}