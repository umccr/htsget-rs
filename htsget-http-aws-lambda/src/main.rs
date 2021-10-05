extern crate jemallocator;

#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

use bytes::Bytes;
use std::io::Cursor;

use lambda_runtime::{handler_fn, Context, Error};
use serde_json::{json, Value};

use aws_sdk_s3 as s3;
use s3::Region;

use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::fmt::SubscriberBuilder;

use htsget_http_core::get_service_info_json as get_base_service_info_json;
use htsget_http_core::Endpoint;
use htsget_search::htsget::HtsGet;

use crate::handlers::fill_out_service_info_json;
use crate::handlers::pretty_json::PrettyJson;
use crate::AsyncAppState;

/// Gets the JSON to return for a service-info endpoint
pub fn get_service_info_json<H: HtsGet + Send + Sync + 'static>(
  app_state: &AsyncAppState<H>,
  endpoint: Endpoint,
) -> impl Responder {
  PrettyJson(fill_out_service_info_json(
    get_base_service_info_json(endpoint, app_state.htsget.clone()),
    &app_state.config,
  ))
}

/// Gets the JSON to return for the reads service-info endpoint
pub async fn reads_service_info<H: HtsGet + Send + Sync + 'static>(
  app_state: Data<AsyncAppState<H>>,
) -> impl Responder {
  get_service_info_json(app_state.get_ref(), Endpoint::Reads)
}

/// Gets the JSON to return for the variants service-info endpoint
pub async fn variants_service_info<H: HtsGet + Send + Sync + 'static>(
  app_state: Data<AsyncAppState<H>>,
) -> impl Responder {
  get_service_info_json(app_state.get_ref(), Endpoint::Variants)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
  let config = envy::from_env::<Config>().expect("The environment variables weren't properly set!");
  let address = format!("{}:{}", config.htsget_ip, config.htsget_port);
  let htsget_path = config.htsget_path.clone();
  let regex_match = config.htsget_regex_match.clone();
  let regex_substitution = config.htsget_regex_substitution.clone();
  
  lambda_runtime::run(handler_fn()).await?;
  Ok(())
}
