use std::sync::Arc;

use htsget_http::get_service_info_json as get_base_service_info_json;
use htsget_http::Endpoint;
use htsget_search::htsget::HtsGet;

use crate::handlers::FormatJson;
use crate::ServiceInfo;
use worker::{Response};

/// Service info endpoint.
pub fn get_service_info_json<H: HtsGet + Send + Sync + 'static>(
  searcher: Arc<H>,
  endpoint: Endpoint,
  config: &ServiceInfo,
) -> http::Result<Response> {
  FormatJson(get_base_service_info_json(endpoint, searcher, config)).try_into()
}
