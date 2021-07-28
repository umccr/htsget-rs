use crate::AppState;

use super::Config;
use actix_web::{dev::HttpResponseBuilder, http::StatusCode, web::Data, HttpResponse, Responder};
use htsget_http_core::{
  get_service_info_json as get_base_service_info_json, Endpoint, JsonResponse, Result, ServiceInfo,
};
use htsget_search::htsget::HtsGet;

pub mod get;
pub mod post;

/// Handles a response, converting errors to json and using the proper HTTP status code
fn handle_response(response: Result<JsonResponse>) -> impl Responder {
  match response {
    Err(error) => {
      let (json, status_code) = error.to_json_representation();
      HttpResponseBuilder::new(StatusCode::from_u16(status_code).unwrap()).json(json)
    }
    Ok(json) => HttpResponseBuilder::new(StatusCode::OK).json(json),
  }
}

/// Gets the JSON to return for a service-info endpoint
fn get_service_info_json<H: HtsGet>(app_state: &AppState<H>, endpoint: Endpoint) -> impl Responder {
  HttpResponse::Ok().json(fill_out_service_info_json(
    get_base_service_info_json(endpoint, &app_state.htsget),
    &app_state.config,
  ))
}

/// Fills the service-info json with the data from the server config
fn fill_out_service_info_json(mut service_info_json: ServiceInfo, config: &Config) -> ServiceInfo {
  if let Some(id) = &config.htsget_id {
    service_info_json.id = id.clone();
  }
  if let Some(name) = &config.htsget_name {
    service_info_json.name = name.clone();
  }
  if let Some(version) = &config.htsget_version {
    service_info_json.version = version.clone();
  }
  if let Some(organization_name) = &config.htsget_organization_name {
    service_info_json.organization.name = organization_name.clone();
  }
  if let Some(organization_url) = &config.htsget_organization_url {
    service_info_json.organization.url = organization_url.clone();
  }
  service_info_json
}

/// Gets the JSON to return for the reads service-info endpoint
pub async fn reads_service_info<H: HtsGet>(app_state: Data<AppState<H>>) -> impl Responder {
  get_service_info_json(app_state.get_ref(), Endpoint::Reads)
}

/// Gets the JSON to return for the variants service-info endpoint
pub async fn variants_service_info<H: HtsGet>(app_state: Data<AppState<H>>) -> impl Responder {
  get_service_info_json(app_state.get_ref(), Endpoint::Variants)
}
