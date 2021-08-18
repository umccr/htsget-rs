use actix_web::{web::Data, Responder};

use htsget_http_core::blocking::service_info::get_service_info_json as base_service_info_json;
use htsget_http_core::Endpoint;
use htsget_search::htsget::blocking::HtsGet;

use crate::handlers::fill_out_service_info_json;
use crate::handlers::pretty_json::PrettyJson;
use crate::AppState;

pub mod get;
pub mod post;

/// Gets the JSON to return for a service-info endpoint
fn get_service_info_json<H: HtsGet>(app_state: &AppState<H>, endpoint: Endpoint) -> impl Responder {
  PrettyJson(fill_out_service_info_json(
    base_service_info_json(endpoint, &app_state.htsget),
    &app_state.config,
  ))
}

/// Gets the JSON to return for the reads service-info endpoint
pub async fn reads_service_info<H: HtsGet>(app_state: Data<AppState<H>>) -> impl Responder {
  get_service_info_json(app_state.get_ref(), Endpoint::Reads)
}

/// Gets the JSON to return for the variants service-info endpoint
pub async fn variants_service_info<H: HtsGet>(app_state: Data<AppState<H>>) -> impl Responder {
  get_service_info_json(app_state.get_ref(), Endpoint::Variants)
}
