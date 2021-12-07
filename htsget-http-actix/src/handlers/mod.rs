use actix_web::{http::StatusCode, Either, Responder};

use htsget_http_core::{JsonResponse, Result, ServiceInfo};
use pretty_json::PrettyJson;

#[cfg(feature = "async")]
pub use crate::handlers::async_handlers::{
  get_service_info_json, reads_service_info, variants_service_info,
};

use super::Config;

#[cfg(feature = "async")]
pub mod async_handlers;
pub mod blocking;
#[cfg(feature = "async")]
pub mod get;
#[cfg(feature = "async")]
pub mod post;

mod pretty_json;

/// Handles a response, converting errors to json and using the proper HTTP status code
fn handle_response(response: Result<JsonResponse>) -> Either<impl Responder, impl Responder> {
  match response {
    Err(error) => {
      let (json, status_code) = error.to_json_representation();
      Either::Left(PrettyJson(json).with_status(StatusCode::from_u16(status_code).unwrap()))
    }
    Ok(json) => Either::Right(PrettyJson(json).with_status(StatusCode::OK)),
  }
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
  if let Some(contact_url) = &config.htsget_contact_url {
    service_info_json.contact_url = contact_url.clone();
  }
  if let Some(documentation_url) = &config.htsget_documentation_url {
    service_info_json.documentation_url = documentation_url.clone();
  }
  if let Some(created_at) = &config.htsget_created_at {
    service_info_json.created_at = created_at.clone();
  }
  if let Some(updated_at) = &config.htsget_updated_at {
    service_info_json.updated_at = updated_at.clone();
  }
  if let Some(environment) = &config.htsget_environment {
    service_info_json.environment = environment.clone();
  }
  service_info_json
}
