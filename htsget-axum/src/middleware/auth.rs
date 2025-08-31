//! Authentication middleware for htsget-axum.
//!

use crate::error::HtsGetResult;
use crate::middleware::extract_request;
use axum::extract::Request;
use axum::response::{IntoResponse, Response};
use futures::future::BoxFuture;
use htsget_http::middleware::auth::Auth;
use std::task::{Context, Poll};
use tower::{Layer, Service};

impl From<Auth> for AuthLayer {
  fn from(auth: Auth) -> Self {
    Self { inner: auth }
  }
}

/// A wrapper around the authorization layer.
#[derive(Clone)]
pub struct AuthLayer {
  inner: Auth,
}

impl AuthLayer {
  /// Get the inner auth layer.
  pub fn inner(&self) -> &Auth {
    &self.inner
  }
}

impl<S> Layer<S> for AuthLayer {
  type Service = AuthMiddleware<S>;

  fn layer(&self, inner: S) -> Self::Service {
    AuthMiddleware::new(inner, self.clone())
  }
}

/// A wrapper around the authorization middleware.
#[derive(Clone)]
pub struct AuthMiddleware<S> {
  inner: S,
  layer: AuthLayer,
}

impl<S> AuthMiddleware<S> {
  /// Create a new middleware auth.
  pub fn new(inner: S, layer: AuthLayer) -> Self {
    Self { inner, layer }
  }

  /// Get the inner service.
  pub fn inner(&self) -> &S {
    &self.inner
  }

  /// Get the layer.
  pub fn layer(&self) -> &AuthLayer {
    &self.layer
  }

  /// Validate the request using the htsget-http validator.
  pub async fn validate_authorization(&self, request: &mut Request) -> HtsGetResult<()> {
    let mut htsget_request = extract_request(request).await?;
    let suppressed_request = self
      .layer
      .inner
      .authorize_request(&mut htsget_request)
      .await?;

    request.extensions_mut().insert(suppressed_request);
    Ok(())
  }
}

impl<S> Service<Request> for AuthMiddleware<S>
where
  S: Service<Request, Response = Response> + Clone + Send + 'static + Sync,
  S::Future: Send + 'static,
{
  type Response = S::Response;
  type Error = S::Error;
  type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

  fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    self.inner.poll_ready(cx)
  }

  fn call(&mut self, mut request: Request) -> Self::Future {
    let clone = self.clone();
    // The inner service must be ready so we replace it with the cloned value.
    // See https://docs.rs/tower/latest/tower/trait.Service.html#be-careful-when-cloning-inner-services
    let mut self_owned = std::mem::replace(self, clone);
    Box::pin(async move {
      if let Err(err) = self_owned.validate_authorization(&mut request).await {
        return Ok(err.into_response());
      }

      self_owned.inner.call(request).await
    })
  }
}
