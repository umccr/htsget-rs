use std::sync::Arc;

use lambda_http::IntoResponse;

use htsget_config::config::HtsgetConfig;
use htsget_http_core::get_service_info_json as get_base_service_info_json;
use htsget_http_core::Endpoint;
use htsget_search::htsget::HtsGet;

use crate::handlers::FormatJson;

/// Return the Json service info response.
pub fn get_service_info_json<H: HtsGet + Send + Sync + 'static>(
  searcher: Arc<H>,
  endpoint: Endpoint,
  config: &HtsgetConfig,
) -> impl IntoResponse {
  FormatJson(get_base_service_info_json(endpoint, searcher, config))
}
