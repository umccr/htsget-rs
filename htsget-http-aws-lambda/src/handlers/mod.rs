pub mod async_handlers;
pub mod pretty_json;

use pretty_json::PrettyJson;

use htsget_http_core::{ServiceInfo};

pub use crate::handlers::async_handlers::{
    get_service_info_json
};

use super::Config;

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