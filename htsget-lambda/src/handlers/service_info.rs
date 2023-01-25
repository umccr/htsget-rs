use std::sync::Arc;

use lambda_http::http;
use tracing::info;
use tracing::instrument;

use htsget_http::get_service_info_json as get_base_service_info_json;
use htsget_http::Endpoint;
use htsget_search::htsget::HtsGet;

use crate::handlers::FormatJson;
use crate::ServiceInfo;
use crate::{Body, Response};

/// Service info endpoint.
#[instrument(skip(searcher))]
pub fn get_service_info_json<H: HtsGet + Send + Sync + 'static>(
  searcher: Arc<H>,
  endpoint: Endpoint,
  config: &ServiceInfo,
) -> http::Result<Response<Body>> {
  info!(endpoint = ?endpoint, "service info request");
  FormatJson(get_base_service_info_json(endpoint, searcher, config)).try_into()
}
