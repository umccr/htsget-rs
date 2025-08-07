//! Authentication middleware for htsget-actix.
//!

use crate::handlers::{HttpVersionCompat, extract_request_path};
use actix_web::body::{BoxBody, EitherBody};
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::web::Query;
use actix_web::{Error, FromRequest, HttpResponse};
use axum::body::to_bytes;
use axum::response::IntoResponse;
use futures_util::future::LocalBoxFuture;
use htsget_axum::error::HtsGetError;
use htsget_http::Endpoint;
use htsget_http::middleware::auth::Auth;
use std::collections::HashMap;
use std::future::{Ready, ready};
use std::sync::Arc;
use std::task::{Context, Poll};

/// A wrapper around the axum middleware layer.
#[derive(Clone)]
pub struct AuthLayer(pub Auth);

impl<S, B> Transform<S, ServiceRequest> for AuthLayer
where
  S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
  S::Future: 'static,
  B: 'static,
{
  type Response = ServiceResponse<EitherBody<B, BoxBody>>;
  type Error = Error;
  type Transform = AuthMiddleware<S>;
  type InitError = ();
  type Future = Ready<Result<Self::Transform, Self::InitError>>;

  fn new_transform(&self, service: S) -> Self::Future {
    ready(Ok(AuthMiddleware::new(Arc::new(service), self.0.clone())))
  }
}

/// A wrapper around the axum middleware.
pub struct AuthMiddleware<S> {
  service: Arc<S>,
  inner: Auth,
}

impl<S> Clone for AuthMiddleware<S> {
  fn clone(&self) -> Self {
    Self::new(self.service.clone(), self.inner.clone())
  }
}

impl<S> AuthMiddleware<S> {
  /// Create a new middleware layer.
  pub fn new(service: Arc<S>, inner: Auth) -> Self {
    Self { service, inner }
  }

  /// Validate the authorization.
  pub async fn validate_authorization(&self, req: &mut ServiceRequest) -> Result<(), HtsGetError> {
    let (req, payload) = req.parts_mut();

    let path = req.path();
    let query = <Query<HashMap<String, String>> as FromRequest>::from_request(req, payload)
      .await
      .map_err(|err| HtsGetError::permission_denied(err.to_string()))?;

    let (request, endpoint) = if let Some(reads) = path.strip_prefix("/reads") {
      (
        extract_request_path(query, reads.to_string(), req.clone()),
        Endpoint::Reads,
      )
    } else if let Some(variants) = path.strip_prefix("/variants") {
      (
        extract_request_path(query, variants.to_string(), req.clone()),
        Endpoint::Variants,
      )
    } else {
      // Only authorize on the variants and reads endpoints, no need to service info.
      return Ok(());
    };

    Ok(self.inner.validate_authorization(request, endpoint).await?)
  }
}

impl<S, B> Service<ServiceRequest> for AuthMiddleware<S>
where
  S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
  S::Future: 'static,
  B: 'static,
{
  type Response = ServiceResponse<EitherBody<B, BoxBody>>;
  type Error = Error;
  type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

  fn poll_ready(&self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    self.service.poll_ready(ctx)
  }

  fn call(&self, mut req: ServiceRequest) -> Self::Future {
    let self_owned = self.clone();
    Box::pin(async move {
      if let Err(err) = self_owned.validate_authorization(&mut req).await {
        let response = err.into_response();
        let status_code = response.status();
        let body = to_bytes(response.into_body(), 1000)
          .await
          .map(|bytes| bytes.to_vec())
          .unwrap_or_default();

        return Ok(req.into_response(HttpResponse::with_body(
          HttpVersionCompat::status_code_1_to_0_2(status_code),
          EitherBody::right(BoxBody::new(body)),
        )));
      }

      Ok(self_owned.service.call(req).await?.map_into_left_body())
    })
  }
}
