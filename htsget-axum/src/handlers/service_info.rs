use axum::extract::State;
use axum::response::IntoResponse;
use axum_extra::response::ErasedJson;

use htsget_http::get_service_info_json as get_base_service_info_json;
use htsget_http::Endpoint;
use htsget_search::HtsGet;

use crate::server::AppState;

/// Gets the JSON to return for a service-info endpoint
pub fn get_service_info_json<H: HtsGet + Send + Sync + 'static>(
  app_state: AppState<H>,
  endpoint: Endpoint,
) -> impl IntoResponse {
  ErasedJson::pretty(get_base_service_info_json(
    endpoint,
    app_state.htsget,
    &app_state.service_info,
  ))
}

/// Gets the JSON to return for the reads service-info endpoint
pub async fn reads_service_info<H: HtsGet + Send + Sync + 'static>(
  State(app_state): State<AppState<H>>,
) -> impl IntoResponse {
  get_service_info_json(app_state, Endpoint::Reads)
}

/// Gets the JSON to return for the variants service-info endpoint
pub async fn variants_service_info<H: HtsGet + Send + Sync + 'static>(
  State(app_state): State<AppState<H>>,
) -> impl IntoResponse {
  get_service_info_json(app_state, Endpoint::Variants)
}
