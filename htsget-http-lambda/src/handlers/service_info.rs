use std::sync::Arc;

use lambda_http::http;
use tracing::info;

use htsget_config::config::ConfigServiceInfo;
use htsget_http_core::get_service_info_json as get_base_service_info_json;
use htsget_http_core::Endpoint;
use htsget_search::htsget::HtsGet;

use crate::handlers::FormatJson;
use crate::{Body, Response};

/// Service info endpoint.
pub fn get_service_info_json<H: HtsGet + Send + Sync + 'static>(
  searcher: Arc<H>,
  endpoint: Endpoint,
  config: &ConfigServiceInfo,
) -> http::Result<Response<Body>> {
  info!(endpoint = ?endpoint, "Service info request");
  FormatJson(get_base_service_info_json(endpoint, searcher, config)).try_into()
}
