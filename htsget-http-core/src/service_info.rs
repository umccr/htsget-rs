use crate::Endpoint;
use htsget_search::htsget::HtsGet;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ServiceInfo {
  pub id: String,
  pub name: String,
  pub version: String,
  pub organization: ServiceInfoOrganization,
  #[serde(rename = "type")]
  pub service_type: ServiceInfoType,
  pub htsget: ServiceInfoHtsget,
}

#[derive(Serialize, Deserialize)]
pub struct ServiceInfoOrganization {
  pub name: String,
  pub url: String,
}

#[derive(Serialize, Deserialize)]
pub struct ServiceInfoType {
  pub group: String,
  pub artifact: String,
  pub version: String,
}

#[derive(Serialize, Deserialize)]
pub struct ServiceInfoHtsget {
  pub datatype: String,
  pub formats: Vec<String>,
  #[serde(rename = "fieldsParametersEffective")]
  pub fields_parameters_effective: String,
  #[serde(rename = "TagsParametersEffective")]
  pub tags_parameters_effective: String,
}

pub fn get_service_info_json(endpoint: Endpoint, searcher: &impl HtsGet) -> ServiceInfo {
  let hstget_info = ServiceInfoHtsget {
    datatype: match endpoint {
      Endpoint::Reads => "reads",
      Endpoint::Variants => "variants",
    }
    .to_string(),
    formats: searcher
      .get_supported_formats()
      .iter()
      .map(|format| format.to_string())
      .collect(),
    fields_parameters_effective: searcher.are_field_parameters_effective().to_string(),
    tags_parameters_effective: searcher.are_tag_parameters_effective().to_string(),
  };
  let type_info = ServiceInfoType {
    group: "org.ga4gh".to_string(),
    artifact: "htsget".to_string(),
    version: "1.3.0".to_string(),
  };
  let organization_info = ServiceInfoOrganization {
    name: "Snake oil".to_string(),
    url: "https://en.wikipedia.org/wiki/Snake_oil".to_string(),
  };
  ServiceInfo {
    id: "".to_string(),
    name: "HtsGet service".to_string(),
    version: "".to_string(),
    organization: organization_info,
    service_type: type_info,
    htsget: hstget_info,
  }
}
