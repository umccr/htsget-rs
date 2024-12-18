use actix_web::web::Data;
use actix_web::Responder;
use tracing::info;
use tracing::instrument;

use htsget_http::get_service_info_json as get_base_service_info_json;
use htsget_http::Endpoint;
use htsget_search::HtsGet;

use crate::handlers::pretty_json::PrettyJson;
use crate::AppState;

/// Gets the JSON to return for a service-info endpoint
#[instrument(skip(app_state))]
pub fn get_service_info_json<H: HtsGet + Clone + Send + Sync + 'static>(
  app_state: &AppState<H>,
  endpoint: Endpoint,
) -> impl Responder {
  info!(endpoint = ?endpoint, "service info request");

  PrettyJson(get_base_service_info_json(
    endpoint,
    app_state.htsget.clone(),
    app_state.config_service_info.clone(),
  ))
}

/// Gets the JSON to return for the reads service-info endpoint
pub async fn reads_service_info<H: HtsGet + Clone + Send + Sync + 'static>(
  app_state: Data<AppState<H>>,
) -> impl Responder {
  get_service_info_json(app_state.get_ref(), Endpoint::Reads)
}

/// Gets the JSON to return for the variants service-info endpoint
pub async fn variants_service_info<H: HtsGet + Clone + Send + Sync + 'static>(
  app_state: Data<AppState<H>>,
) -> impl Responder {
  get_service_info_json(app_state.get_ref(), Endpoint::Variants)
}
