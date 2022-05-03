use std::sync::Arc;

use lambda_http::IntoResponse;

use htsget_config::config::{Config, ConfigServiceInfo};
use htsget_http_core::get_service_info_json as get_base_service_info_json;
use htsget_http_core::Endpoint;
use htsget_search::htsget::HtsGet;

use crate::handlers::FormatJson;

/// Service info endpoint.
pub fn get_service_info_json<H: HtsGet + Send + Sync + 'static>(
  searcher: Arc<H>,
  endpoint: Endpoint,
  config: &ConfigServiceInfo,
) -> impl IntoResponse {
  FormatJson(get_base_service_info_json(endpoint, searcher, config))
}
