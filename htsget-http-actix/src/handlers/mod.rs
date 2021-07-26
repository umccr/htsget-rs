use crate::AppState;

use super::Config;
use actix_web::{
  http::StatusCode,
  web::{Data, Json},
  Responder,
};
use htsget_http_core::{
  get_service_info_json as get_base_service_info_json, Endpoint, Result, ServiceInfo,
};
use htsget_search::htsget::HtsGet;

pub mod get;
pub mod post;

fn handle_response(response: Result<String>) -> impl Responder {
  match response {
    Err(error) => {
      let (json, status_code) = error.to_json_representation();
      Json(json).with_status(StatusCode::from_u16(status_code).unwrap())
    }
    Ok(json) => Json(json).with_status(StatusCode::OK),
  }
}

fn get_service_info_json<H: HtsGet>(app_state: &AppState<H>, endpoint: Endpoint) -> impl Responder {
  Json(fill_out_service_info_json(
    get_base_service_info_json(endpoint, &app_state.htsget),
    &app_state.config,
  ))
}

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

pub async fn reads_service_info<H: HtsGet>(app_state: Data<AppState<H>>) -> impl Responder {
  get_service_info_json(app_state.get_ref(), Endpoint::Reads)
}

pub async fn variants_service_info<H: HtsGet>(app_state: Data<AppState<H>>) -> impl Responder {
  get_service_info_json(app_state.get_ref(), Endpoint::Variants)
}
