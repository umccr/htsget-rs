use actix_web::web::Data;
use actix_web::Responder;

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
