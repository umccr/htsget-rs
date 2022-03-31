use lambda_http::{Body, IntoResponse, Request, Response, Error};
use lambda_http::http::Method;
use htsget_config::config::HtsgetConfig;
use lambda_http::RequestExt;
use regex::Regex;

pub async fn lambda_function(request: Request, config: &HtsgetConfig) -> Result<impl IntoResponse, Error> {
  let uri = request.uri().path();
  match *request.method() {
    Method::GET => {
      unimplemented!()
    },
    Method::POST => {
      unimplemented!()
    },
    _ => Ok(Response::builder().status(405).body("").unwrap())
  }
}

/// Regex which matches the relevant parts of a htsget request.
pub fn regex_path() -> Regex {
  Regex::new(r"^/(?P<endpoint>reads|variants)/(?:(?P<service_info>service-info$)|(?P<id>.+$))").expect("Expected valid regex pattern.")
}

#[cfg(test)]
mod tests {
  use lambda_http::http::Uri;
  use crate::regex_path;

  #[test]
  fn test_regex_no_endpoint() {
    let regex = regex_path();
    let uri = Uri::builder().path_and_query("/path/").build().unwrap();
    assert!(!regex.is_match(uri.path()));
  }

  #[test]
  fn test_regex_reads_no_id() {
    let regex = regex_path();
    let uri = Uri::builder().path_and_query("/reads/").build().unwrap();
    assert!(!regex.is_match(uri.path()));
  }

  #[test]
  fn test_regex_variants_no_id() {
    let regex = regex_path();
    let uri = Uri::builder().path_and_query("/variants/").build().unwrap();
    assert!(!regex.is_match(uri.path()));
  }

  #[test]
  fn test_regex_reads_service_info() {
    let regex = regex_path();
    let uri = Uri::builder().path_and_query("/reads/service-info").build().unwrap();
    let match_regex = regex.captures(uri.path()).unwrap();
    assert_eq!(match_regex.name("endpoint").unwrap().as_str(), "reads");
    assert_eq!(match_regex.name("service_info").unwrap().as_str(), "service-info");
    assert!(match_regex.name("id").is_none());
  }

  #[test]
  fn test_regex_variants_service_info() {
    let regex = regex_path();
    let uri = Uri::builder().path_and_query("/variants/service-info").build().unwrap();
    let match_regex = regex.captures(uri.path()).unwrap();
    assert_eq!(match_regex.name("endpoint").unwrap().as_str(), "variants");
    assert_eq!(match_regex.name("service_info").unwrap().as_str(), "service-info");
    assert!(match_regex.name("id").is_none());
  }

  #[test]
  fn test_regex_reads_id() {
    let regex = regex_path();
    let uri = Uri::builder().path_and_query("/reads/id").build().unwrap();
    let match_regex = regex.captures(uri.path()).unwrap();
    assert_eq!(match_regex.name("endpoint").unwrap().as_str(), "reads");
    assert!(match_regex.name("service_info").is_none());
    assert_eq!(match_regex.name("id").unwrap().as_str(), "id");
  }

  #[test]
  fn test_regex_variants_id() {
    let regex = regex_path();
    let uri = Uri::builder().path_and_query("/variants/id").build().unwrap();
    let match_regex = regex.captures(uri.path()).unwrap();
    assert_eq!(match_regex.name("endpoint").unwrap().as_str(), "variants");
    assert!(match_regex.name("service_info").is_none());
    assert_eq!(match_regex.name("id").unwrap().as_str(), "id");
  }
}