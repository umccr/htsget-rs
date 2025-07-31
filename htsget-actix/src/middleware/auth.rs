//! Authentication middleware for htsget-actix.
//!

use crate::handlers::HttpVersionCompat;
use actix_web::body::{BoxBody, EitherBody};
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{Error, HttpResponse};
use axum::body::to_bytes;
use axum::response::IntoResponse;
use futures_util::future::LocalBoxFuture;
use headers::authorization::Bearer;
use headers::{Authorization, Header};
use htsget_axum::error::HtsGetError;
use htsget_http::middleware::auth::Auth;
use http_1::HeaderMap;
use std::future::{ready, Ready};
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
  pub async fn validate_authorization(
    &self,
    header_map: HeaderMap,
  ) -> htsget_axum::error::HtsGetResult<()> {
    let auth_token = Authorization::<Bearer>::decode(&mut header_map.values())
      .map_err(|err| HtsGetError::invalid_authentication(err.to_string()))?;
    Ok(
      self
        .inner
        .validate_authorization(auth_token.token())
        .await?,
    )
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

  fn call(&self, req: ServiceRequest) -> Self::Future {
    let self_owned = self.clone();
    Box::pin(async move {
      let header_map = HttpVersionCompat::header_map_0_2_to_1(req.headers().clone().into());

      if let Err(err) = self_owned.validate_authorization(header_map).await {
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

      self_owned.call(req).await
    })
  }
}
