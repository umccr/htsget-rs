#[cfg(not(feature = "async"))]
use actix_web::{web::Data, Responder};

#[cfg(not(feature = "async"))]
use htsget_http_core::blocking::service_info::get_service_info_json as base_service_info_json;
#[cfg(not(feature = "async"))]
use htsget_http_core::Endpoint;
#[cfg(not(feature = "async"))]
use htsget_search::htsget::blocking::HtsGet;

#[cfg(not(feature = "async"))]
use crate::handlers::fill_out_service_info_json;
#[cfg(not(feature = "async"))]
use crate::handlers::pretty_json::PrettyJson;
#[cfg(not(feature = "async"))]
use crate::AppState;

pub mod get;
pub mod post;

/// Gets the JSON to return for a service-info endpoint
#[cfg(not(feature = "async"))]
fn get_service_info_json<H: HtsGet>(app_state: &AppState<H>, endpoint: Endpoint) -> impl Responder {
  PrettyJson(fill_out_service_info_json(
    base_service_info_json(endpoint, &app_state.htsget),
    &app_state.config,
  ))
}

/// Gets the JSON to return for the reads service-info endpoint
#[cfg(not(feature = "async"))]
pub async fn reads_service_info<H: HtsGet>(app_state: Data<AppState<H>>) -> impl Responder {
  get_service_info_json(app_state.get_ref(), Endpoint::Reads)
}

/// Gets the JSON to return for the variants service-info endpoint
#[cfg(not(feature = "async"))]
pub async fn variants_service_info<H: HtsGet>(app_state: Data<AppState<H>>) -> impl Responder {
  get_service_info_json(app_state.get_ref(), Endpoint::Variants)
}
