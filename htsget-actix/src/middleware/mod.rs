//! Middleware for htsget-actix
//!

use crate::handlers::{HeaderMap, HttpVersionCompat};
use actix_utils::future::{Ready, ok};
use actix_web::dev::ServiceRequest;
use actix_web::web::Query;
use actix_web::{Error, FromRequest, HttpMessage};
use htsget_axum::error::{HtsGetError, HtsGetResult};
use htsget_config::types;
use std::collections::HashMap;

pub mod auth;

/// A request type for middleware logic that implements `FromRequest`.
#[derive(Clone, Debug)]
pub struct SuppressedRequest(pub(crate) Option<types::SuppressedRequest>);

impl FromRequest for SuppressedRequest {
  type Error = Error;
  type Future = Ready<Result<Self, Self::Error>>;

  fn from_request(req: &actix_web::HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
    if let Some(suppressed_request) = req.extensions().get::<Self>() {
      ok(suppressed_request.clone())
    } else {
      ok(SuppressedRequest(None))
    }
  }
}

pub(crate) async fn extract_request(req: &mut ServiceRequest) -> HtsGetResult<types::Request> {
  let (req, payload) = req.parts_mut();

  let query = <Query<HashMap<String, String>> as FromRequest>::from_request(req, payload)
    .await
    .map_err(|err| HtsGetError::permission_denied(err.to_string()))?;
  let headers = HttpVersionCompat::header_map_0_2_to_1(HeaderMap::from(&req.clone()).into_inner());
  let path = req.path();

  Ok(types::Request::new(path.to_string(), query.0, headers))
}
